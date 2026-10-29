//! The window, drawn with Rune in Hokum's look (`rune_theme_opencode`). It reads
//! [`State`] snapshots from a [`Bluetooth`] handle and sends
//! [`Command`](crate::model::Command)s; it never touches D-Bus.
//!
//! A placeholder for now: the UI agent replaces the view. Keep `spec()`, `run()` and the
//! `mock` scene (scripts/build-linux.sh renders it as the release smoke test).

use std::sync::Arc;

use rune::{AppSpec, Cx, El, WindowOptions, col, text};
use rune_kit::Kit;

use crate::config::{Appearance, Config};
use crate::model::{Bluetooth, State, Subscription};
use crate::update::Available;

pub const SIZE: (f32, f32) = (400.0, 560.0);

pub struct Ui {
    pub bt: Arc<State>,
    /// `None` in headless scenes.
    #[allow(dead_code)]
    pub handle: Option<Bluetooth>,
    pub config: Config,
    /// A newer build, once the check-in found one: the UI's "update available" affordance.
    pub update: Option<Available>,
    _subscription: Option<Subscription>,
}

pub fn init() -> Ui {
    Ui { bt: Arc::new(crate::mock::sample()), handle: None, config: crate::config::load(), update: None, _subscription: None }
}

fn kit(s: &Ui, cx: &Cx<Ui>) -> &'static Kit {
    match s.config.appearance {
        Appearance::System => rune_theme_opencode::kit(cx.system_dark()),
        Appearance::Light => rune_theme_opencode::kit(false),
        Appearance::Dark => rune_theme_opencode::kit(true),
    }
}

fn view(s: &Ui, cx: &mut Cx<Ui>) -> El {
    let k = kit(s, cx);
    let connected = s.bt.connected().map(|d| d.display_name().to_owned()).collect::<Vec<_>>().join(", ");
    col()
        .key("window")
        .w_full()
        .h_full()
        .padding(24.0)
        .gap(8.0)
        .fill(k.c.page)
        .child(text("Hokum Bluetooth").key("title").font(k.ty.ui_medium.lh(16.0)).color(k.c.text_muted))
        .child(text(connected).key("connected").font(k.ty.ui_medium.lh(16.0)))
}

fn scene_mock(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::sample());
}

fn scene_scan(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::scanning());
}

fn scene_pairing(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::pairing());
}

fn scene_off(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::powered_off());
}

fn scene_no_bluez(s: &mut Ui) {
    s.bt = Arc::new(crate::mock::not_running());
}

pub fn spec() -> AppSpec<Ui> {
    AppSpec {
        scenes: &[("mock", scene_mock), ("scan", scene_scan), ("pairing", scene_pairing), ("off", scene_off), ("no-bluez", scene_no_bluez)],
        fonts: rune_theme_opencode::FONTS,
        families: rune_theme_opencode::FAMILIES,
        ..AppSpec::new("Hokum Bluetooth", SIZE, init, view)
    }
}

pub fn window() -> WindowOptions {
    WindowOptions { min_size: Some((360.0, 360.0)), ..WindowOptions::default() }
}

/// Opens the window on `bt` until it closes. Update check-ins land in `Ui::update`.
pub fn run(bt: Bluetooth) {
    rune::set_app_id("hokum-bluetooth");
    let app = rune::App::new(spec()).window_options(window()).args(Vec::new());
    let handle = app.handle();
    let sub = {
        let handle = handle.clone();
        bt.on_change(move |s| {
            let s = s.clone();
            handle.update(move |ui, _| ui.bt = s);
        })
    };
    {
        let handle = handle.clone();
        crate::update::spawn(move |a| handle.update(move |ui, _| ui.update = Some(a)));
    }
    app.init(move || Ui { bt: bt.state(), handle: Some(bt), config: crate::config::load(), update: None, _subscription: Some(sub) }).run();
}
