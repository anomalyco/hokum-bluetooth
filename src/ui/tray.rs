//! `blue service`: a tray icon (`rune::tray`, StatusNotifierItem on Linux) whose click opens
//! the first screen as a small popover, and the full window on demand, in one process. Its menu
//! lists connected devices with battery and connects paired ones in one click. The same
//! approach as Hokum Sound's tray.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rune::{Align, KeyPolicy, OverlayInput, OverlayOptions, OverlayPlacement, OverlaySpec, ScreenEdge, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec};

use super::{Mode, POPOVER, POPOVER_MARGIN, Ui};
use crate::model::{Bluetooth, Command, DeviceId, State};

/// Tall enough for the first screen with a row open; outside the card, clicks pass through.
const HEIGHT: f32 = 720.0;

/// Bluetooth's rune, white on transparent, drawn at `size` pixels (dimmed when off).
pub fn icon(size: u32, on: bool) -> TrayIcon {
    let s = size as f32 / 16.0;
    let pts = [[4.5, 5.0], [11.5, 11.0], [8.0, 14.0], [8.0, 2.0], [11.5, 5.0], [4.5, 11.0]];
    let near = |x: f32, y: f32| {
        let (x, y) = (x / s, y / s);
        pts.windows(2).any(|w| {
            let ([ax, ay], [bx, by]) = (w[0], w[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            let (px, py) = (ax + t * dx - x, ay + t * dy - y);
            px * px + py * py < 0.85 * 0.85
        })
    };
    let alpha = if on { 255 } else { 120 };
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    if near(px as f32 + (sx as f32 + 0.5) / 4.0, py as f32 + (sy as f32 + 0.5) / 4.0) {
                        hits += 1;
                    }
                }
            }
            rgba.extend_from_slice(&[255, 255, 255, (hits * alpha / 16) as u8]);
        }
    }
    TrayIcon { width: size, height: size, rgba }
}

fn powered(s: &State) -> bool {
    s.default_adapter().is_some_and(|a| a.powered)
}

fn tooltip(s: &State) -> String {
    if !powered(s) {
        return "Bluetooth off".into();
    }
    let names: Vec<String> = s
        .connected()
        .map(|d| match d.battery {
            Some(b) => format!("{} {b}%", d.display_name()),
            None => d.display_name().to_owned(),
        })
        .collect();
    if names.is_empty() { "Bluetooth".into() } else { names.join(" · ") }
}

/// Connected devices (with battery) to read, paired ones to connect, then Open and Quit.
fn menu(s: &State) -> Vec<TrayItem> {
    let mut items = Vec::new();
    if powered(s) {
        for d in s.connected() {
            let label = match d.battery {
                Some(b) => format!("{} · {b}%", d.display_name()),
                None => d.display_name().to_owned(),
            };
            items.push(TrayItem::action(format!("disconnect:{}", d.id), label));
        }
        let paired: Vec<_> = s.paired().collect();
        if !items.is_empty() && !paired.is_empty() {
            items.push(TrayItem::Separator);
        }
        for d in paired {
            items.push(TrayItem::action(format!("connect:{}", d.id), format!("Connect {}", d.display_name())).enabled(d.pending.is_none()));
        }
        if !items.is_empty() {
            items.push(TrayItem::Separator);
        }
    }
    items.push(TrayItem::action("open", "Open Bluetooth"));
    items.push(TrayItem::Separator);
    items.push(TrayItem::action("quit", "Quit"));
    items
}

fn shape(items: &[TrayItem]) -> String {
    items
        .iter()
        .map(|i| match i {
            TrayItem::Action { id, label, enabled } => format!("{id}\u{1f}{label}\u{1f}{enabled}"),
            TrayItem::Separator => "-".into(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs the tray, the popover and the (hidden) window until Quit.
pub fn service(bt: Bluetooth) {
    rune::set_app_id("hokum-bluetooth");
    let mut app = rune::App::new(super::spec()).window_options(super::window()).fit_content_height().windowless().on_close(|s: &mut Ui, _| s.hidden()).args(Vec::new());
    let window = app.handle();

    let mut popover = Ui::new(Some(bt.clone()), Mode::Popover);
    popover.visible = false;
    let spec = OverlaySpec::new("hokum-bluetooth", (POPOVER + 2.0 * POPOVER_MARGIN, HEIGHT), super::view).options(OverlayOptions {
        placement: Some(OverlayPlacement::Edge { edge: ScreenEdge::Top, margin: 4.0, align: Align::End, inset: 4.0 }),
        keys: KeyPolicy::OnDemand,
        input: OverlayInput::Regions,
        ..OverlayOptions::default()
    });
    let spec = spec.on_key(super::on_key).on_focus(|s: &mut Ui, focused, _| {
        if !focused
            && s.visible
            && let Some(d) = &s.dismiss
        {
            d();
        }
    });
    let overlay = app.overlay(spec, popover);

    let shown = Arc::new(AtomicBool::new(false));
    let hide: Arc<dyn Fn() + Send + Sync> = {
        let (overlay, shown) = (overlay.clone(), shown.clone());
        Arc::new(move || {
            shown.store(false, Ordering::SeqCst);
            overlay.hide();
            overlay.update(|ui, _| ui.hidden());
        })
    };
    let open_window: Arc<dyn Fn() + Send + Sync> = {
        let window = window.clone();
        Arc::new(move || {
            window.update(|ui, _| ui.visible = true);
            window.open();
            window.focus();
        })
    };
    {
        let (hide, open_window) = (hide.clone(), open_window.clone());
        overlay.update(move |ui, _| {
            ui.dismiss = Some(hide);
            ui.open_window = Some(open_window);
        });
    }
    let toggle = {
        let (overlay, shown, hide) = (overlay.clone(), shown.clone(), hide.clone());
        move || {
            if shown.swap(true, Ordering::SeqCst) {
                hide();
                return;
            }
            overlay.update(|ui, _| {
                ui.visible = true;
                ui.open = None;
                ui.page = super::Page::Main;
            });
            overlay.show();
            overlay.request_keys();
        }
    };

    let state = bt.state();
    let tray = {
        let (open_window, window, bt) = (open_window.clone(), window.clone(), bt.clone());
        let spec = TraySpec { id: "hokum-bluetooth".into(), title: "Bluetooth".into(), tooltip: tooltip(&state), icon: icon(32, powered(&state)), menu: menu(&state) };
        match Tray::new(spec, move |e| match e {
            TrayEvent::Activate { .. } => toggle(),
            TrayEvent::Menu(id) if id == "open" => open_window(),
            TrayEvent::Menu(id) if id == "quit" => window.quit(),
            TrayEvent::Menu(id) => {
                if let Some(path) = id.strip_prefix("connect:") {
                    bt.send(Command::Connect { device: DeviceId(path.into()) });
                } else if let Some(path) = id.strip_prefix("disconnect:") {
                    bt.send(Command::Disconnect { device: DeviceId(path.into()) });
                }
            }
        }) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                eprintln!("blue: no tray icon: {e}");
                None
            }
        }
    };

    let last = Mutex::new((tooltip(&state), shape(&menu(&state))));
    let subscription = {
        let (window, overlay, tray) = (window.clone(), overlay.clone(), tray.clone());
        bt.on_change(move |s| {
            if let Some(t) = &tray {
                let (tip, items) = (tooltip(s), menu(s));
                let shape = shape(&items);
                let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
                if last.0 != tip {
                    t.set_tooltip(tip.clone());
                    last.0 = tip;
                }
                if last.1 != shape {
                    t.set_menu(items);
                    last.1 = shape;
                }
            }
            let (a, b) = (s.clone(), s.clone());
            window.update(move |ui, _| ui.receive(a));
            overlay.update(move |ui, _| ui.receive(b));
        })
    };
    {
        let window = window.clone();
        crate::update::spawn(move |a| window.update(move |ui, _| ui.update = Some(a)));
    }
    app.init(move || {
        let mut ui = Ui::new(Some(bt), Mode::Window);
        ui.visible = false;
        ui._subscription = Some(Arc::new(subscription));
        ui.tray = tray;
        ui
    })
    .run();
}
