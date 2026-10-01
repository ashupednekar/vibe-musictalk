#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$script_dir/.."
# Build the static Dioxus client; Wrangler builds the Rust signaling Worker.
dx build --platform web --release --fullstack false --no-default-features --features web
cd crates/signaling
wrangler deploy
