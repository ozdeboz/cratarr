#!/bin/sh
# The players' rules for the page (the 0075 spike, 2026-10-30; split in 0.12): server/rules built to WebAssembly in the
# server's own Rust image (no wasm-bindgen: plain exports, no imports), three times: the brake into
# web/src/lib/wasm/rules.wasm (loaded with the page, its base64 copy in rulesWasm.ts), the matcher into
# web/src/lib/wasm/matcher.wasm (a file the page fetches when idle), and the library on the device into
# web/src/lib/wasm/index.wasm (a file the page fetches when it first syncs or reads it). Only that last carries the Unicode
# normalization tables, as what it searches lies in its bytes: for the others the page folds texts itself (matcher.rs).
set -eu
cd "$(dirname "$0")/.."
image=$(tr -d '\r' < Dockerfile | sed -n 's/^FROM \(rust:[^ ]*\) AS source$/\1/p')
docker run --rm -v "$(pwd):/src" -w /src/server -e CARGO_TARGET_DIR=/tmp/t "$image" sh -c '
  set -e
  rustup target add wasm32-unknown-unknown >/dev/null
  build() {
    cargo build -q -p cratarr-rules --no-default-features --features "$1" --target wasm32-unknown-unknown --profile wasm
    cp /tmp/t/wasm32-unknown-unknown/wasm/cratarr_rules.wasm "/src/web/src/lib/wasm/$2"
  }
  build wasm-brake rules.wasm
  build wasm-matcher,wasm-spread matcher.wasm
  build wasm-index index.wasm
'
ls -l web/src/lib/wasm/rules.wasm web/src/lib/wasm/matcher.wasm web/src/lib/wasm/index.wasm
