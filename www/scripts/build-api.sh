#!/usr/bin/env sh
set -eu
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SITE_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
DIST_DIR=${SYSTEMLESS_PAGES_DIST:-"$SITE_DIR/dist"}
# Pin the generator together with the worker crate. Cargo skips matching installs.
cargo install worker-build --version 0.8.7 --locked
(cd "$SITE_DIR/api" && RUSTFLAGS= worker-build --release -- --locked)
mkdir -p "$DIST_DIR/_worker.js"
cp "$SITE_DIR/api/pages.js" "$DIST_DIR/_worker.js/index.js"
cp "$SITE_DIR/api/build/index.js" "$DIST_DIR/_worker.js/rust.js"
cp "$SITE_DIR/api/build/index_bg.wasm" "$DIST_DIR/_worker.js/"
