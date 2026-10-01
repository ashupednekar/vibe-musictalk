#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
export MUSICTALK_WEB_SERVER_URL="https://musictalk.ashupednekar49.workers.dev"
dx build --platform web --release --fullstack false --no-default-features --features web
wrangler pages deploy target/dx/musictalk/release/web/public --project-name musictalk --branch main --commit-dirty=true
