//! The BlueZ backend: talks to `org.bluez` on the system bus with zbus on its own thread,
//! keeps a [`State`] of adapters, devices (with `Battery1`) and the pairing agent's requests,
//! publishes it through a [`Driver`](crate::model::Driver) and carries out
//! [`Command`](crate::model::Command)s. See `crate::model` for the contract.
//!
//! Not written yet: the backend agent fills this module in.

#[allow(unused_imports)]
use crate::model::{Bluetooth, State};

/// Connects to the system bus and starts the backend thread.
pub fn start() -> Result<Bluetooth, String> {
    let _ = zbus::blocking::Connection::system;
    Err("the BlueZ backend is not implemented yet; run with --mock".into())
}
