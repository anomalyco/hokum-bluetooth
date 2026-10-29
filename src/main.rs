//! Hokum Bluetooth: a Bluetooth manager for Linux on BlueZ, drawn with Rune. One of the Hokum
//! tools.
//!
//! ```sh
//! blue                        # open the window
//! blue --mock                 # the window on a fake adapter and devices (no BlueZ)
//! blue service                # stay running in the background (tray; later)
//! blue check                  # report problems in config.kdl
//! blue update [--install]     # check for a new build now (and install it)
//! blue --version
//! blue --snap /tmp/blue.png --scene mock   # headless render on the mock backend
//! ```
//!
//! `crate::model` is the contract between the backend (`crate::bluez`, BlueZ over D-Bus;
//! `crate::mock`, fake) and the window (`crate::ui`).

mod bluez;
// Parts of the contract the backend and the window don't use yet.
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod mock;
#[allow(dead_code)]
mod model;
mod ui;
#[allow(dead_code)]
mod update;

const HELP: &str = "usage: blue [service | check | update] [--mock] [--version] [--help]
  blue                   open the window
    --mock               use a fake adapter and devices instead of BlueZ
                         (or HOKUM_BLUETOOTH_BACKEND=mock)
  blue service           stay running in the background (tray; not yet)
  blue check             print problems in ~/.config/hokum-bluetooth/config.kdl;
                         exits non-zero if there are any
  blue update            check for a new build on this build's channel now
    --install            and install it, for a plain writable install
  --version              print the version
Rune's tooling flags (--snap, --scene, --size, --theme, ...) render headless on the mock
backend; scenes: mock, scan, pairing, off, no-bluez.";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--version" || a == "-V") {
        println!("blue {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    match argv.first().map(String::as_str) {
        Some("check") => std::process::exit(if config::check() { 0 } else { 1 }),
        Some("update") => std::process::exit(update::cli(&argv)),
        Some("debug") => std::process::exit(bluez::debug::cli(&argv)),
        Some("service") => {
            eprintln!("blue: the background service is not implemented yet");
            std::process::exit(1);
        }
        _ => {}
    }
    let headless = argv.iter().any(|a| {
        matches!(a.as_str(), "--snap" | "--tree" | "--trace" | "--frames" | "--strip" | "--list-scenes" | "--first-pixels")
    });
    if headless {
        rune::App::new(ui::spec()).run();
        return;
    }
    let mock = argv.iter().any(|a| a == "--mock") || std::env::var("HOKUM_BLUETOOTH_BACKEND").is_ok_and(|b| b == "mock");
    let source = if mock { model::Source::Mock } else { model::Source::Bluez };
    match model::start(source) {
        Ok(bt) => ui::run(bt),
        Err(e) => {
            eprintln!("blue: {e}");
            std::process::exit(1);
        }
    }
}
