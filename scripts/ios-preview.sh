#!/bin/sh
# Requires full Xcode, an accepted license, and an installed iOS runtime.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
export MUSICTALK_SERVER_URL="${MUSICTALK_SERVER_URL:-http://127.0.0.1:8080}"
xcrun simctl list devices available
# Use a caller-selected simulator, or the first available iPhone.
if [ -z "${MUSICTALK_SIMULATOR:-}" ]; then
    MUSICTALK_SIMULATOR=$(xcrun simctl list devices available | sed -n 's/.*iPhone.*(\([0-9A-F-]*\)) (.*)/\1/p' | head -n 1)
fi
if [ -z "$MUSICTALK_SIMULATOR" ]; then
    printf 'No iPhone simulator is installed. Add an iOS runtime in Xcode Settings > Components.\n' >&2
    exit 1
fi
if ! xcrun simctl list devices | grep -F "$MUSICTALK_SIMULATOR" | grep -q '(Booted)'; then xcrun simctl boot "$MUSICTALK_SIMULATOR"; fi
open -a Simulator
xcrun simctl bootstatus "$MUSICTALK_SIMULATOR" -b
dx build --platform ios --target aarch64-apple-ios-sim --device "$MUSICTALK_SIMULATOR" --no-default-features --features mobile --fullstack false
# Dioxus emits the .app within its iOS bundle output.
app_path=$(find target/dx/musictalk/debug/ios -name '*.app' -type d | head -n 1)
if [ -z "$app_path" ]; then printf 'No iOS .app was produced.\n' >&2; exit 1; fi
xcrun simctl install "$MUSICTALK_SIMULATOR" "$app_path"
xcrun simctl launch "$MUSICTALK_SIMULATOR" dev.musictalk
