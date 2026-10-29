# Hokum Bluetooth Agent Guide

Hokum Bluetooth (`blue`) is a Linux-only Bluetooth manager — a replacement for
blueman, blueberry and bluetui — drawn with Rune on native BlueZ over D-Bus
(zbus). One of the Hokum tools (anomalyco/hokum is the notifications app,
anomalyco/hokum-sound the audio app): package `hokum-<tool>`, plain binary
name, config in `~/.config/hokum-<tool>/`.

## Speed

- Move fast and fix forward: merge once it compiles; wait only for checks
  that gate the merge.
- No fixed sleeps to wait on CI: use `gh run watch <id> --exit-status`.
- Never end your turn while still waiting on something.
- Dev releases never cancel each other: a running release always finishes,
  and pushes in between are skipped so only the newest pending run goes next
  (`cancel-in-progress: false` on Release's per-channel group). Never add
  `cancel-in-progress: true` or a cancel step to a build or publish workflow.

## Design

- Minimal. Every control has to earn its place.
- Progressive disclosure:
  - First level, what people touch daily: the power toggle; connected devices
    (icon, name, battery, one click to disconnect); paired devices (one click
    to connect).
  - Second level: scan and pair new devices (PIN/passkey prompts inline, not
    in dialogs), trust and forget, rename, and for audio devices a hand-off to
    Hokum Sound (`sound`) for profiles and codecs — never duplicate them here.
  - Third level: adapters, discoverable/pairable and their timeouts, LE
    details, raw properties.
  - Tray (`blue service`): connected devices with battery, quick connect.
- Almost no copy: no explanations, labels only where an icon can't say it.
- At most about six controls on the first level.
- Review every UI change from screenshots (`blue --snap <png> --scene <name>`,
  on the mock backend) before calling it done. Scenes: `main` (= `mock`),
  `expanded`, `scan`, `pairing`, `typing`, `pending`, `advanced`, `devices`,
  `tray`, `off`, `no-bluez`. The window fits its content's height, but a snap
  renders at `--size`: read the height from `--tree`'s `viewport` and pass it.

## Layout

- `src/model.rs` — the contract: `State` snapshots, `Command`s, the
  `Bluetooth` (UI side) / `Driver` (backend side) channel. Change it
  deliberately; both the backend and the UI build on it.
- `src/bluez/` — the BlueZ backend (`bluez::start() -> Bluetooth`): Adapter1,
  Device1, Battery1 and the pairing agent (Agent1) over zbus.
- `src/mock.rs` — a fake adapter and devices with timed connects, discovery
  and pairing prompts; `--mock` and every headless scene.
- `src/ui/` — the Rune window (`view.rs` levels one and two, `advanced.rs`
  level three) and `tray.rs` (`blue service`: `rune::Tray` StatusNotifierItem
  plus the popover, as Hokum Sound does it); keep the `mock` scene (the
  release smoke test renders it).
- `src/update.rs` — the release check-in (`hokum/<channel>/<version>/bluetooth`).
- `src/config.rs` — `~/.config/hokum-bluetooth/config.kdl`.
- Releases: docs/releasing.md. `.github/` needs Dax.

## Testing on real hardware

Test against real BlueZ on `apollo` (Dax's desktop) over SSH; romulus has no
Bluetooth radio worth trusting.

- Never unpair, forget, block or untrust Dax's real devices. Pair and forget
  only devices you paired yourself for the test.
- Leave the adapter as you found it: note `Powered`, `Discoverable`,
  `Pairable` and their timeouts and the alias first (`bluetoothctl show`),
  and restore them when done, even after a failure.
- Don't disconnect devices Dax is using (his headphones mid-call, his
  keyboard and mouse) without asking.
- Stop any discovery you started.

## Building

Plain Rust: zbus speaks D-Bus itself, so no C headers are needed. `cargo
test`, `cargo build --release`, then `target/release/blue --snap
/tmp/blue.png --scene mock`.
