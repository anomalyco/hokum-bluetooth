//! The pairing agent (`org.bluez.Agent1`) and who owns the default agent.
//!
//! Every `blue` process that talks to BlueZ (the window and the tray service) exports and
//! registers its own agent. BlueZ asks the agent of the connection that called `Pair` first, so
//! a pairing started from the window always prompts in the window. Requests nobody started
//! here (an incoming pairing, a paired but untrusted device asking for a service) go to the
//! *default* agent:
//!
//! - `blue service` always calls `RequestDefaultAgent` and holds an exclusive `flock` on
//!   `$XDG_RUNTIME_DIR/hokum-bluetooth-service.lock` while it runs.
//! - The window calls `RequestDefaultAgent` only when that lock is free (no service runs). If a
//!   service is running it waits for the lock to come free and claims the default then.
//! - BlueZ keeps default agents in a stack: when the service starts after the window it takes
//!   over, and when it exits the window's agent is the default again.
//!
//! Each agent call becomes [`Event::Agent`] for the backend loop, which turns it into
//! `State::pairing` and answers through a channel when `Command::PairingReply` arrives (or a
//! timeout rejects it).

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use zbus::message::Header;
use zbus::zvariant::OwnedObjectPath;

use super::Event;
use crate::model::Reply;

pub const PATH: &str = "/ly/anoma/hokum/bluetooth/agent";
pub const CAPABILITY: &str = "KeyboardDisplay";

/// What an agent call asks.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    PinCode,
    DisplayPinCode(String),
    Passkey,
    DisplayPasskey { passkey: u32, entered: u16 },
    Confirmation(u32),
    Authorization,
    AuthorizeService(String),
}

pub struct Call {
    /// The caller's unique bus name; only BlueZ's is honoured.
    pub sender: String,
    pub device: String,
    pub ask: Ask,
    /// Where the answer goes, for calls that wait on one. Dropping it cancels the call.
    pub answer: Option<async_channel::Sender<Reply>>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

pub struct Agent {
    pub events: Sender<Event>,
}

fn sender(h: &Header<'_>) -> String {
    h.sender().map(|s| s.to_string()).unwrap_or_default()
}

impl Agent {
    fn tell(&self, h: &Header<'_>, device: OwnedObjectPath, ask: Ask) {
        let _ = self.events.send(Event::Agent(Call { sender: sender(h), device: device.to_string(), ask, answer: None }));
    }

    async fn ask(&self, h: &Header<'_>, device: OwnedObjectPath, ask: Ask) -> Result<Reply, AgentError> {
        let (tx, rx) = async_channel::bounded(1);
        let call = Call { sender: sender(h), device: device.to_string(), ask, answer: Some(tx) };
        if self.events.send(Event::Agent(call)).is_err() {
            return Err(AgentError::Canceled("agent stopped".into()));
        }
        match rx.recv().await {
            Ok(Reply::Reject) => Err(AgentError::Rejected("rejected".into())),
            Ok(r) => Ok(r),
            Err(_) => Err(AgentError::Canceled("cancelled".into())),
        }
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {
        let _ = self.events.send(Event::AgentReleased);
    }

    async fn request_pin_code(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath) -> Result<String, AgentError> {
        match self.ask(&h, device, Ask::PinCode).await? {
            Reply::Pin(p) if !p.is_empty() && p.len() <= 16 => Ok(p),
            _ => Err(AgentError::Rejected("no PIN".into())),
        }
    }

    async fn display_pin_code(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath, pincode: String) -> Result<(), AgentError> {
        self.tell(&h, device, Ask::DisplayPinCode(pincode));
        Ok(())
    }

    async fn request_passkey(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath) -> Result<u32, AgentError> {
        match self.ask(&h, device, Ask::Passkey).await? {
            Reply::Passkey(p) if p <= 999_999 => Ok(p),
            _ => Err(AgentError::Rejected("no passkey".into())),
        }
    }

    async fn display_passkey(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath, passkey: u32, entered: u16) {
        self.tell(&h, device, Ask::DisplayPasskey { passkey, entered });
    }

    async fn request_confirmation(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath, passkey: u32) -> Result<(), AgentError> {
        self.ask(&h, device, Ask::Confirmation(passkey)).await.map(|_| ())
    }

    async fn request_authorization(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath) -> Result<(), AgentError> {
        self.ask(&h, device, Ask::Authorization).await.map(|_| ())
    }

    async fn authorize_service(&self, #[zbus(header)] h: Header<'_>, device: OwnedObjectPath, uuid: String) -> Result<(), AgentError> {
        self.ask(&h, device, Ask::AuthorizeService(uuid)).await.map(|_| ())
    }

    async fn cancel(&self, #[zbus(header)] h: Header<'_>) {
        let _ = self.events.send(Event::AgentCancel(sender(&h)));
    }
}

fn lock_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d).join("hokum-bluetooth-service.lock"),
        _ => std::env::temp_dir().join(format!("hokum-bluetooth-service-{}.lock", unsafe { libc::getuid() })),
    }
}

fn open() -> Option<File> {
    OpenOptions::new().create(true).truncate(false).read(true).write(true).open(lock_path()).ok()
}

fn flock(f: &File, op: libc::c_int) -> bool {
    unsafe { libc::flock(f.as_raw_fd(), op) == 0 }
}

/// Marks this process as the running service while the returned file stays open. `None` if
/// another service already runs.
pub fn hold_service_lock() -> Option<File> {
    let f = open()?;
    flock(&f, libc::LOCK_EX | libc::LOCK_NB).then_some(f)
}

/// Whether a `blue service` is running.
pub fn service_running() -> bool {
    let Some(f) = open() else { return false };
    if flock(&f, libc::LOCK_SH | libc::LOCK_NB) {
        flock(&f, libc::LOCK_UN);
        false
    } else {
        true
    }
}

/// Blocks until no service runs (or holds the lock for a service, with `exclusive`), then
/// returns. Run on its own thread.
pub fn wait_for_lock(exclusive: bool) -> Option<File> {
    let f = open()?;
    let op = if exclusive { libc::LOCK_EX } else { libc::LOCK_SH };
    if !flock(&f, op) {
        return None;
    }
    if !exclusive {
        flock(&f, libc::LOCK_UN);
    }
    Some(f)
}
