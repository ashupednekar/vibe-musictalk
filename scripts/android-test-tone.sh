#!/bin/sh
# Install a tiny, independent app with explicit audio capture eligibility.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
. "$script_dir/android-env.sh"
fixture_dir="${TMPDIR:-/tmp}/musictalk-test-tone"
mkdir -p "$fixture_dir/classes" "$fixture_dir/dex"
android_jar="$ANDROID_HOME/platforms/android-35/android.jar"
build_tools="$ANDROID_HOME/build-tools/35.0.0"
javac -source 8 -target 8 -bootclasspath "$android_jar" -d "$fixture_dir/classes" "$project_dir/tests/fixtures/tone/ToneActivity.java"
"$build_tools/d8" --lib "$android_jar" --output "$fixture_dir/dex" "$fixture_dir"/classes/dev/musictalk/testtone/*.class
"$build_tools/aapt" package -f -M "$project_dir/tests/fixtures/tone/AndroidManifest.xml" -I "$android_jar" -F "$fixture_dir/unsigned.apk"
cd "$fixture_dir/dex"
"$build_tools/aapt" add "$fixture_dir/unsigned.apk" classes.dex
if [ ! -f "$fixture_dir/test.keystore" ]; then
    keytool -genkeypair -alias testtone -keyalg RSA -validity 30 -keystore "$fixture_dir/test.keystore" -storepass android -keypass android -dname 'CN=MusicTalk audio fixture'
fi
"$build_tools/apksigner" sign --ks "$fixture_dir/test.keystore" --ks-pass pass:android --out "$fixture_dir/tone.apk" "$fixture_dir/unsigned.apk"
adb install -r "$fixture_dir/tone.apk"
