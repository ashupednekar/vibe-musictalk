#!/bin/sh
# Xcode owns provisioning and signing. Dioxus builds the bundled native executable.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
cd "$project_dir"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
export MUSICTALK_SERVER_URL="${MUSICTALK_SERVER_URL:-https://musictalk.ashupednekar49.workers.dev}"
case "$PLATFORM_NAME" in
    iphoneos) rust_target=aarch64-apple-ios ;;
    iphonesimulator)
        case "${CURRENT_ARCH:-arm64}" in
            x86_64) rust_target=x86_64-apple-ios ;;
            *) rust_target=aarch64-apple-ios-sim ;;
        esac ;;
    *) printf 'Unsupported Xcode platform: %s\n' "$PLATFORM_NAME" >&2; exit 1 ;;
esac
# Let Rust select the SDK per target, including Dioxus's host-side build.
unset SDKROOT SDK_DIR
dx build --platform ios --target "$rust_target" --release --no-default-features --features mobile --fullstack false
rust_app="$project_dir/target/dx/musictalk/release/ios/Musictalk.app"
app_dir="$TARGET_BUILD_DIR/$WRAPPER_NAME"
mkdir -p "$app_dir"
cp "$rust_app/musictalk" "$app_dir/$EXECUTABLE_NAME"
chmod +x "$app_dir/$EXECUTABLE_NAME"
ditto "$rust_app/assets" "$app_dir/assets"
