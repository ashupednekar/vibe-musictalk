#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
. "$script_dir/android-env.sh"
export MUSICTALK_SERVER_URL="${MUSICTALK_SERVER_URL:-https://musictalk.ashupednekar49.workers.dev}"
if [ -d /Library/Developer/CommandLineTools ]; then export DEVELOPER_DIR=/Library/Developer/CommandLineTools; fi
dx build --platform android --release --no-default-features --features mobile --fullstack false
sh "$script_dir/android-package.sh"
