#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
simulator=${MUSICTALK_SIMULATOR:-F769F222-DF4E-4686-A1F9-4940D91C2BAB}
sdk_path=$(xcrun --sdk iphonesimulator --show-sdk-path)
output=${TMPDIR:-/private/tmp}/musictalk-ios-audio-test
xcrun --sdk iphonesimulator clang -target arm64-apple-ios15.0-simulator -isysroot "$sdk_path" -fobjc-arc -fblocks tests/ios-audio.m -framework Foundation -framework ReplayKit -framework AVFoundation -framework CoreMedia -framework Security -o "$output"
codesign --force --sign - "$output"
xcrun simctl spawn "$simulator" "$output"
