#!/bin/sh
# The players' rules for the page (the 0075 spike, 2026-10-30): server/rules built to WebAssembly in the server's own
# Rust image (no wasm-bindgen: plain exports of numbers, no imports), written to web/src/lib/wasm/rules.wasm.
set -eu
cd "$(dirname "$0")/.."
image=$(sed -n 's/^FROM \(rust:[^ ]*\) AS build$/\1/p' Dockerfile)
docker run --rm -v "$(pwd):/src" -w /src/server -e CARGO_TARGET_DIR=/tmp/t "$image" sh -c \
  'rustup target add wasm32-unknown-unknown >/dev/null && cargo build -q -p cratarr-rules --target wasm32-unknown-unknown --release && cp /tmp/t/wasm32-unknown-unknown/release/cratarr_rules.wasm /src/web/src/lib/wasm/rules.wasm'
ls -l web/src/lib/wasm/rules.wasm
