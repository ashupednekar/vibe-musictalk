#!/bin/sh
# Apply launcher resources after Dioxus generates its Android project.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
. "$script_dir/android-env.sh"
project="$PWD/target/dx/musictalk/release/android/app"
resources="$project/app/src/main/res"
cp -R native/android/res/. "$resources/"
mkdir -p "$resources/drawable-nodpi"
cp assets/app-icon.png "$resources/drawable-nodpi/musictalk_icon.png"
# All supported devices are API 29+, so use the adaptive launcher resource.
sed 's|@mipmap/ic_launcher|@mipmap/musictalk_launcher|g' native/android/AndroidManifest.xml > "$project/app/src/main/AndroidManifest.xml"
cd "$project"
sh gradlew --offline --no-daemon assembleDebug
cd "$script_dir/.."
mkdir -p dist
cp "$project/app/build/outputs/apk/debug/app-debug.apk" dist/MusicTalk.apk
