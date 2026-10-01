#!/bin/sh
# Read-only report. Installation and licensing stay explicit.
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$script_dir/android-env.sh"
printf '\nAndroid SDK\n'
if command -v adb >/dev/null 2>&1; then adb version; adb devices -l; else printf 'adb is missing\n'; fi
if command -v emulator >/dev/null 2>&1; then emulator -list-avds; else printf 'Android emulator is missing\n'; fi
printf '\niOS SDK\n'
if [ -d /Applications/Xcode.app ]; then
    DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -version
    DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun simctl list devices available
else
    printf 'Full Xcode is not installed. Install Xcode from the Mac App Store and add an iOS runtime.\n'
fi
printf '\nRust compilation targets\n'
rustup target list --installed
