#!/bin/sh
# Publishes Hokum Bluetooth (app hokum, artifact bluetooth) to the Anomaly release service through its shared client,
# https://anoma.ly/release/client.sh (anomalyco/release docs/publishing.md).
# The client trades the job's GitHub OIDC token for a one-hour publish
# session, uploads in parallel (skipping files already stored), retries with
# backoff and checks the published files. Runs only in the Release
# workflow's publish jobs, which have `id-token: write`.
#
#   release-service.sh publish <dist> <notes-url>   # upload every target, then the release
#   release-service.sh distribution <kind> <version>  # a package manager now serves <version>
#
# RELEASE is https://anoma.ly/release; HOKUM_VERSION the version; HOKUM_RELEASE_CHANNEL
# the channel (default stable). `publish` writes <dist>/manifest.json. A rerun
# of the channel's current version succeeds when it is exactly these files.
set -eu

: "${RELEASE:?Set RELEASE to the release service origin}"
: "${HOKUM_VERSION:?Set HOKUM_VERSION}"
channel=${HOKUM_RELEASE_CHANNEL:-stable}
case "$RELEASE" in
  https://anoma.ly/release) ;;
  *) echo "RELEASE must be https://anoma.ly/release." >&2; exit 1 ;;
esac

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
curl -fsSL --retry 5 --retry-all-errors "$RELEASE/client.sh" -o "$temporary/client.sh"
client() { RELEASE=$RELEASE bash "$temporary/client.sh" "$@"; }

# The package managers that mirror Hokum Bluetooth, each as the service lists it.
distributions='[
  {"kind": "aur", "package": "hokum-bluetooth-bin", "command": "yay -Syu hokum-bluetooth-bin"},
  {"kind": "deb-x64", "package": "hokum-bluetooth", "command": "curl -fLo /tmp/hokum-bluetooth.deb https://anoma.ly/release/hokum/bluetooth/stable/linux-x64-deb && sudo apt install /tmp/hokum-bluetooth.deb"},
  {"kind": "deb-arm64", "package": "hokum-bluetooth", "command": "curl -fLo /tmp/hokum-bluetooth.deb https://anoma.ly/release/hokum/bluetooth/stable/linux-arm64-deb && sudo apt install /tmp/hokum-bluetooth.deb"},
  {"kind": "rpm-x64", "package": "hokum-bluetooth", "command": "curl -fLo /tmp/hokum-bluetooth.rpm https://anoma.ly/release/hokum/bluetooth/stable/linux-x64-rpm && sudo dnf install /tmp/hokum-bluetooth.rpm"},
  {"kind": "rpm-arm64", "package": "hokum-bluetooth", "command": "curl -fLo /tmp/hokum-bluetooth.rpm https://anoma.ly/release/hokum/bluetooth/stable/linux-arm64-rpm && sudo dnf install /tmp/hokum-bluetooth.rpm"}
]'

publish() {
  dist=$1
  notes_url=$2
  version=$HOKUM_VERSION
  # target=local file=published name, as docs/releasing.md pins.
  # Published names carry no version or target: the sha256 in the path tells
  # files apart. A run without a platform's signed build publishes the others.
  set --
  for entry in \
    "linux-x64=hokum-bluetooth-$version-linux-x64=blue" \
    "linux-x64-deb=hokum-bluetooth-$version-linux-x64.deb=hokum-bluetooth.deb" \
    "linux-x64-rpm=hokum-bluetooth-$version-linux-x64.rpm=hokum-bluetooth.rpm" \
    "linux-arm64=hokum-bluetooth-$version-linux-arm64=blue" \
    "linux-arm64-deb=hokum-bluetooth-$version-linux-arm64.deb=hokum-bluetooth.deb" \
    "linux-arm64-rpm=hokum-bluetooth-$version-linux-arm64.rpm=hokum-bluetooth.rpm"
  do
    target=${entry%%=*}
    rest=${entry#*=}
    local_name=${rest%%=*}
    name=${rest#*=}
    [ -f "$dist/$local_name" ] || continue
    set -- "$@" --target "$target=$dist/$local_name=$name"
  done
  [ $# -gt 0 ] || { echo "No release files in $dist." >&2; exit 1; }
  jq -n --arg channel "$channel" --arg version "$version" --arg notes_url "$notes_url" \
    --argjson distributions "$distributions" \
    '{channel: $channel, version: $version, notes_url: $notes_url,
      distributions: ([{kind: "direct"}] + $distributions)}' > "$temporary/release.json"
  client publish --app hokum --artifact bluetooth --release "$temporary/release.json" \
    --out "$dist/manifest.json" --verify-downloads "$@"
}

distribution() {
  # The whole entry, so recording a version never drops its command.
  entry=$(printf '%s' "$distributions" | jq -ec --arg kind "$1" --arg version "$2" \
    'map(select(.kind == $kind))[0] // error("unknown distribution \($kind)") | . + {version: $version}')
  client distribution --app hokum --artifact bluetooth --channel "$channel" --entry "$entry"
}

case "${1:-}" in
  publish)
    [ $# -eq 3 ] || { echo "Usage: $0 publish <dist> <notes-url>" >&2; exit 1; }
    publish "$2" "$3"
    ;;
  distribution)
    [ $# -eq 3 ] || { echo "Usage: $0 distribution <kind> <version>" >&2; exit 1; }
    distribution "$2" "$3"
    ;;
  *)
    echo "Usage: $0 publish <dist> <notes-url> | distribution <kind> <version>" >&2
    exit 1
    ;;
esac
