#!/bin/sh
# The players' rules for the Android app (Spike B, 0.11): server/rules-ffi built for the app's ABIs (arm64-v8a, x86_64)
# with cargo-ndk into android/app/build/rustJniLibs (built, never kept in the repository), and its Kotlin written by
# UniFFI to android/app/src/main/java/app/cratarr/rules/. In Docker, like rules-wasm.sh: the server's Rust on Debian (the
# NDK's tools want glibc), the NDK and cargo-ndk kept in the volumes cratarr-android-ndk and cargo-android, so they are
# fetched once. An app built without the library keeps its own Kotlin rules: the switch finds none and stays off.
set -eu
cd "$(dirname "$0")/.."
rust=$(tr -d '\r' < Dockerfile | sed -n 's/^FROM rust:\([0-9.]*\)-alpine.* AS source$/\1/p')
ndk=r27c
docker run --rm -v "$(pwd):/src" -v cratarr-android-ndk:/opt/ndk -v cargo-android:/usr/local/cargo/registry \
  -v cargo-android-bin:/opt/cargo-bin -w /src/server/rules-ffi -e CARGO_TARGET_DIR=/tmp/t -e NDK=$ndk \
  "rust:$rust-bookworm" sh -euc '
  if [ ! -d /opt/ndk/android-ndk-$NDK ]; then
    curl -sSfL -o /tmp/ndk.zip https://dl.google.com/android/repository/android-ndk-$NDK-linux.zip
    unzip -q /tmp/ndk.zip -d /opt/ndk && rm /tmp/ndk.zip
  fi
  export ANDROID_NDK_HOME=/opt/ndk/android-ndk-$NDK PATH=/opt/cargo-bin/bin:$PATH
  rustup target add aarch64-linux-android x86_64-linux-android >/dev/null 2>&1
  command -v cargo-ndk >/dev/null || cargo install -q --locked --root /opt/cargo-bin cargo-ndk
  cargo ndk -t arm64-v8a -t x86_64 -o /src/android/app/build/rustJniLibs build --release
  # The rules crate is a cdylib too (for the page): only the bridge goes in the app.
  rm -f /src/android/app/build/rustJniLibs/*/libcratarr_rules.so
  # The Kotlin from the metadata in the bridge, which the release build strips: read from an unstripped host build.
  cargo build -q --lib
  cargo run -q --bin uniffi-bindgen -- generate --library /tmp/t/debug/libcratarr_rules_ffi.so \
    --language kotlin --no-format --out-dir /src/android/app/src/main/java
  chown -R "$(stat -c %u:%g /src)" /src/android/app/build/rustJniLibs /src/android/app/src/main/java/app/cratarr/rules
'
ls -l android/app/build/rustJniLibs/*/ android/app/src/main/java/app/cratarr/rules/
