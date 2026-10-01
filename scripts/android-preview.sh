#!/bin/sh
# Build and launch the installed native app. Start the backend separately.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
. "$script_dir/android-env.sh"
export MUSICTALK_SERVER_URL="${MUSICTALK_SERVER_URL:-https://musictalk.ashupednekar49.workers.dev}"
# A licensed Command Line Tools installation also supports Android builds.
if [ -d /Library/Developer/CommandLineTools ]; then export DEVELOPER_DIR=/Library/Developer/CommandLineTools; fi
sh "$script_dir/android-build.sh"
adb wait-for-device
adb reverse tcp:8080 tcp:8080
adb install -r target/dx/musictalk/release/android/app/app/build/outputs/apk/debug/app-debug.apk
adb shell am force-stop dev.musictalk
adb shell am start -n dev.musictalk/dev.dioxus.main.MainActivity
