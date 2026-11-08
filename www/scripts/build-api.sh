#!/usr/bin/env sh
set -eu
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SITE_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
DIST_DIR=${SYSTEMLESS_PAGES_DIST:-"$SITE_DIR/dist"}
# Pin the generator together with the worker crate. Cargo skips matching installs.
cargo install worker-build --version 0.8.7 --locked
# Match Cargo.lock and use worker-build's supported local CLI setting. Building
# from the locked crate avoids depending on GitHub's precompiled release assets.
API_BINDGEN_VERSION=0.2.129
if [ -z "${WASM_BINDGEN_BIN:-}" ]; then
    cargo install wasm-bindgen-cli --version "$API_BINDGEN_VERSION" --locked
    WASM_BINDGEN_BIN=$(command -v wasm-bindgen)
    export WASM_BINDGEN_BIN
fi
if [ "$("$WASM_BINDGEN_BIN" --version)" != "wasm-bindgen $API_BINDGEN_VERSION" ]; then
    echo "API build requires wasm-bindgen $API_BINDGEN_VERSION: $WASM_BINDGEN_BIN" >&2
    exit 1
fi
(cd "$SITE_DIR/api" && RUSTFLAGS= worker-build --release -- --locked)
mkdir -p "$DIST_DIR/_worker.js"
cp "$SITE_DIR/api/pages.js" "$DIST_DIR/_worker.js/index.js"
cp "$SITE_DIR/api/build/index.js" "$DIST_DIR/_worker.js/rust.js"
cp "$SITE_DIR/api/build/index_bg.wasm" "$DIST_DIR/_worker.js/"
