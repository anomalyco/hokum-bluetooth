//! The window and the tray popover, drawn with Rune in Hokum's look (`rune_theme_opencode`).
//! Both read [`State`] snapshots from a [`Bluetooth`] handle and send [`Command`]s; neither
//! touches D-Bus.
//!
//! Progressive disclosure:
//! 1. `view::main`: the power switch, connected devices (a click disconnects), paired devices
//!    (a click connects). That's all.
//! 2. A row opens in place: rename, trust, forget, Hokum Sound for audio devices. "Add device"
//!    scans in place, and pairing codes show under the device being paired.
//! 3. `advanced`: adapters and their settings, every device's LE details and raw properties.
//!
//! Keep the `mock` scene: scripts/build-linux.sh renders it as the release smoke test.

mod advanced;
mod glyphs;
pub mod tray;
mod view;

use std::collections::HashSet;
use std::sync::Arc;

use rune::{AppSpec, Cx, El, Key, KeyEvent, TextEdit, Tx, WindowOptions};
use rune_kit::Kit;

use crate::config::{Appearance, Config};
use crate::model::{Bluetooth, Command, DeviceId, PairingKind, Pending, State, Subscription};
use crate::update::Available;

/// The window's width; its height follows the content.
pub const WIDTH: f32 = 380.0;
pub const SIZE: (f32, f32) = (WIDTH, 360.0);
/// The tray popover's width.
pub const POPOVER: f32 = 320.0;
/// Room around the popover's card for its shadow.
pub const POPOVER_MARGIN: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Window,
    Popover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Adapter,
    Devices,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Advanced(Tab),
}

pub struct Ui {
    pub bt: Arc<State>,
    /// `None` in headless scenes.
    pub handle: Option<Bluetooth>,
    pub config: Config,
    /// A newer build, once the check-in found one.
    pub update: Option<Available>,
    pub mode: Mode,
    pub visible: bool,
    pub page: Page,
    /// The device row that's open (level two).
    pub open: Option<DeviceId>,
    /// The open row's name, being edited.
    pub rename: TextEdit,
    /// "Add device" is open: discovery runs while it is.
    pub adding: bool,
    /// A PIN or passkey being typed for the pairing request.
    pub code: TextEdit,
    /// The open select menu, by key.
    pub menu: Option<String>,
    /// The adapter's alias, being edited (advanced).
    pub alias: Option<TextEdit>,
    /// Devices whose details are open on the advanced page.
    pub details: HashSet<DeviceId>,
    /// Hokum Sound (`sound`) is installed: audio devices link to it.
    pub sound: bool,
    /// Opens the full window (the popover's expand button).
    pub open_window: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Hides the popover.
    pub dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// The service's tray icon, kept alive with the window.
    pub tray: Option<Arc<rune::Tray>>,
    _subscription: Option<Arc<Subscription>>,
}

/// Whether `name` is an executable on `PATH`.
fn on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| std::fs::metadata(d.join(name)).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
    })
}

impl Ui {
    pub fn new(bt: Option<Bluetooth>, mode: Mode) -> Ui {
        let state = bt.as_ref().map_or_else(|| Arc::new(crate::mock::sample()), Bluetooth::state);
        Ui {
            bt: state,
            handle: bt,
            config: crate::config::load(),
            update: None,
            mode,
            visible: true,
            page: Page::Main,
            open: None,
            rename: TextEdit::new(""),
            adding: false,
            code: TextEdit::new(""),
            menu: None,
            alias: None,
            details: HashSet::new(),
            sound: on_path("sound"),
            open_window: None,
            dismiss: None,
            tray: None,
            _subscription: None,
        }
    }

    pub fn send(&self, c: Command) {
        if let Some(b) = &self.handle {
            b.send(c);
        }
    }

    pub fn receive(&mut self, state: Arc<State>) {
        if self.open.as_ref().is_some_and(|id| state.device(id).is_none()) {
            self.open = None;
        }
        self.details.retain(|id| state.device(id).is_some());
        if state.pairing.as_ref().map(|p| p.id) != self.bt.pairing.as_ref().map(|p| p.id) {
            self.code = TextEdit::new("");
        }
        // A device being paired that isn't in a list on screen opens the scan list.
        if let Some(p) = &state.pairing
            && state.device(&p.device).is_some_and(|d| !d.is_paired())
        {
            self.adding = true;
        }
        let powered = state.default_adapter().is_some_and(|a| a.powered);
        if !powered {
            self.adding = false;
        }
        self.bt = state;
    }

    fn adapter(&self) -> Option<crate::model::AdapterId> {
        self.bt.default_adapter.clone()
    }

    pub fn powered(&self) -> bool {
        self.bt.default_adapter().is_some_and(|a| a.powered)
    }

    pub fn set_powered(&mut self, on: bool) {
        if let Some(adapter) = self.adapter() {
            self.send(Command::SetPowered { adapter, on });
        }
    }

    /// A connected device's row: disconnect; a paired one's: connect.
    pub fn activate(&mut self, id: &DeviceId) {
        let Some(d) = self.bt.device(id) else { return };
        let device = id.clone();
        match d.pending {
            Some(Pending::Connecting) => self.send(Command::Disconnect { device }),
            Some(_) => {}
            None if d.connected => self.send(Command::Disconnect { device }),
            None if d.is_paired() => self.send(Command::Connect { device }),
            None => self.send(Command::Pair { device }),
        }
    }

    pub fn toggle_open(&mut self, id: &DeviceId) {
        if self.open.as_ref() == Some(id) {
            self.open = None;
            return;
        }
        let alias = self.bt.device(id).map(|d| d.display_name().to_owned()).unwrap_or_default();
        self.rename = TextEdit::new(&alias);
        self.open = Some(id.clone());
    }

    pub fn commit_rename(&mut self) {
        let Some(id) = self.open.clone() else { return };
        let Some(d) = self.bt.device(&id) else { return };
        let alias = self.rename.text.trim().to_owned();
        if alias != d.display_name() {
            let reported = d.name.clone().unwrap_or_default();
            let alias = if alias == reported { String::new() } else { alias };
            self.send(Command::SetAlias { device: id, alias });
        }
    }

    pub fn set_adding(&mut self, on: bool) {
        if self.adding == on {
            return;
        }
        self.adding = on;
        let Some(adapter) = self.adapter() else { return };
        if on {
            self.send(Command::StartDiscovery { adapter });
        } else {
            if let Some(p) = self.bt.pairing.clone()
                && self.bt.device(&p.device).is_some_and(|d| !d.is_paired())
            {
                self.send(Command::CancelPair { device: p.device });
            }
            self.send(Command::StopDiscovery { adapter });
        }
    }

    pub fn reply(&mut self, accept: bool) {
        let Some(p) = self.bt.pairing.clone() else { return };
        let reply = if !accept {
            crate::model::Reply::Reject
        } else {
            let typed = self.code.text.trim().to_owned();
            match p.kind {
                PairingKind::RequestPin => crate::model::Reply::Pin(typed),
                PairingKind::RequestPasskey => match typed.parse() {
                    Ok(n) => crate::model::Reply::Passkey(n),
                    Err(_) => return,
                },
                PairingKind::DisplayPasskey { .. } | PairingKind::DisplayPin { .. } => {
                    self.send(Command::CancelPair { device: p.device });
                    return;
                }
                _ => crate::model::Reply::Accept,
            }
        };
        self.send(Command::PairingReply { id: p.id, reply });
    }

    pub fn open_sound(&self) {
        let _ = std::process::Command::new("sound").stdin(std::process::Stdio::null()).spawn();
    }

    pub fn hidden(&mut self) {
        self.visible = false;
        self.menu = None;
        self.set_adding(false);
    }

    /// Esc: close what's open, one layer at a time; then the popover.
    pub fn escape(&mut self, tx: &mut Tx) -> bool {
        if self.menu.take().is_some() || self.alias.take().is_some() {
            return true;
        }
        if let Page::Advanced(_) = self.page {
            tx.instant();
            self.page = Page::Main;
            return true;
        }
        if self.adding {
            self.set_adding(false);
            return true;
        }
        if self.open.take().is_some() {
            return true;
        }
        if self.mode == Mode::Popover
            && let Some(d) = &self.dismiss
        {
            d();
            return true;
        }
        false
    }
}

pub fn kit(s: &Ui, cx: &Cx<Ui>) -> &'static Kit {
    match s.config.appearance {
        Appearance::System => rune_theme_opencode::kit(cx.system_dark()),
        Appearance::Light => rune_theme_opencode::kit(false),
        Appearance::Dark => rune_theme_opencode::kit(true),
    }
}

pub fn view(s: &Ui, cx: &mut Cx<Ui>) -> El {
    let k = kit(s, cx);
    let mut layer = Vec::new();
    let body = match s.page {
        Page::Main => view::main(s, k),
        Page::Advanced(tab) => advanced::page(s, k, tab, &mut layer),
    };
    let root = match s.mode {
        Mode::Window => body.key("window").w_full().fill(k.c.page),
        Mode::Popover => view::popover(k, body),
    };
    let popover = s.mode == Mode::Popover;
    rune::col().key("root").w_full().child(root).children(layer.into_iter().map(|e| if popover { e.hit() } else { e }))
}

pub fn on_key(s: &mut Ui, e: &KeyEvent, tx: &mut Tx) -> bool {
    match &e.key {
        Key::Escape => s.escape(tx),
        Key::Command(',') if s.mode == Mode::Window => {
            tx.instant();
            s.page = if s.page == Page::Main { Page::Advanced(Tab::Adapter) } else { Page::Main };
            true
        }
        _ => false,
    }
}

fn sample(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::sample());
}

fn scene_main(s: &mut Ui) {
    sample(s);
}

fn scene_expanded(s: &mut Ui) {
    sample(s);
    let id = s.bt.connected().find(|d| d.audio).map(|d| d.id.clone());
    if let Some(id) = id {
        s.toggle_open(&id);
    }
}

fn scene_scan(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::scanning());
    s.adding = true;
}

fn scene_pairing(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::pairing());
    s.adding = true;
}

fn scene_typing(s: &mut Ui) {
    let mut st = crate::mock::scanning();
    let kb = st.nearby().find(|d| d.kind == crate::model::Kind::Keyboard).map(|d| d.id.clone());
    if let Some(kb) = kb {
        if let Some(d) = st.devices.iter_mut().find(|d| d.id == kb) {
            d.pending = Some(Pending::Pairing);
        }
        st.pairing = Some(crate::model::PairingRequest { id: 2, device: kb, kind: PairingKind::DisplayPasskey { passkey: 205117, entered: 3 } });
    }
    s.bt = Arc::new(st);
    s.adding = true;
}

fn scene_pending(s: &mut Ui) {
    let mut st = crate::mock::sample();
    if let Some(d) = st.devices.iter_mut().find(|d| d.is_paired() && !d.connected) {
        d.pending = Some(Pending::Connecting);
    }
    st.issues.push(crate::model::Issue { id: 9, severity: crate::model::Severity::Error, message: "Couldn't connect to JBL Flip 6".into(), device: None });
    s.bt = Arc::new(st);
}

fn scene_advanced(s: &mut Ui) {
    sample(s);
    s.page = Page::Advanced(Tab::Adapter);
}

fn scene_devices(s: &mut Ui) {
    sample(s);
    s.page = Page::Advanced(Tab::Devices);
    let id = s.bt.devices().find(|d| d.kind == crate::model::Kind::Mouse).map(|d| d.id.clone());
    s.details.extend(id);
}

fn scene_tray(s: &mut Ui) {
    sample(s);
    s.mode = Mode::Popover;
}

fn scene_off(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::powered_off());
}

fn scene_no_bluez(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::not_running());
}

pub fn spec() -> AppSpec<Ui> {
    AppSpec {
        scenes: &[
            ("mock", scene_main),
            ("main", scene_main),
            ("expanded", scene_expanded),
            ("scan", scene_scan),
            ("pairing", scene_pairing),
            ("typing", scene_typing),
            ("pending", scene_pending),
            ("advanced", scene_advanced),
            ("devices", scene_devices),
            ("tray", scene_tray),
            ("off", scene_off),
            ("no-bluez", scene_no_bluez),
        ],
        fonts: rune_theme_opencode::FONTS,
        families: rune_theme_opencode::FAMILIES,
        on_key: Some(on_key),
        ..AppSpec::new("Bluetooth", SIZE, || Ui::new(None, Mode::Window), view)
    }
}

pub fn window() -> WindowOptions {
    WindowOptions { min_size: Some((340.0, 160.0)), ..WindowOptions::default() }
}

/// Headless renders (`--snap`, `--tree`, ...) on the mock sample.
pub fn headless() {
    rune::App::new(spec()).fit_content_height().run();
}

/// Opens the window on `bt` until it closes. Update check-ins land in `Ui::update`.
pub fn run(bt: Bluetooth) {
    rune::set_app_id("hokum-bluetooth");
    let app = rune::App::new(spec()).window_options(window()).fit_content_height().on_close(|s: &mut Ui, _| s.hidden()).args(Vec::new());
    let handle = app.handle();
    let sub = {
        let handle = handle.clone();
        bt.on_change(move |s| {
            let s = s.clone();
            handle.update(move |ui, _| ui.receive(s));
        })
    };
    {
        let handle = handle.clone();
        crate::update::spawn(move |a| handle.update(move |ui, _| ui.update = Some(a)));
    }
    app.init(move || {
        let mut ui = Ui::new(Some(bt), Mode::Window);
        ui._subscription = Some(Arc::new(sub));
        ui
    })
    .run();
}
