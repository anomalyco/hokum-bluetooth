# Hokum Bluetooth

A Bluetooth manager for Linux, by Anomaly. Power, pairing, connecting and
battery levels on BlueZ, in one small window. One of the Hokum tools.

```sh
blue                  # open the window
blue --mock           # the window on fake devices (no BlueZ)
blue check            # problems in ~/.config/hokum-bluetooth/config.kdl
blue update           # check for a new build
```

## Install

- Arch: `yay -S hokum-bluetooth-bin`
- Debian / Ubuntu: `curl -fLo /tmp/hokum-bluetooth.deb https://anoma.ly/release/hokum/bluetooth/stable/linux-x64-deb && sudo apt install /tmp/hokum-bluetooth.deb`
- Fedora: `curl -fLo /tmp/hokum-bluetooth.rpm https://anoma.ly/release/hokum/bluetooth/stable/linux-x64-rpm && sudo dnf install /tmp/hokum-bluetooth.rpm`

(`linux-arm64-deb` / `linux-arm64-rpm` on aarch64.) Needs `bluetoothd`
(`bluez`) running.

## Config

`~/.config/hokum-bluetooth/config.kdl`:

```kdl
behaviour {
    appearance "system"      // system, light or dark
    check-for-updates #true  // hourly check-in as hokum/<channel>/<version>/bluetooth
}
```

## Build

Needs Rust; nothing else (zbus speaks D-Bus itself).

```sh
cargo build --release
target/release/blue --snap /tmp/blue.png --scene mock
```

Releases: [docs/releasing.md](docs/releasing.md). MIT licensed.
