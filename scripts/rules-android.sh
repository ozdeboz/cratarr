#!/bin/sh
# The players' rules for the Android app: server/rules-ffi built for the app's ABIs (arm64-v8a, x86_64) with cargo-ndk into
# android/app/build/rustJniLibs (built, never kept in the repository). Every build that makes an app runs this (`rulesLibrary`
# in android/app/build.gradle.kts). In Docker, like rules-wasm.sh: the server's Rust on Debian (the NDK's tools want glibc), the
# NDK, cargo-ndk and Rust with the app's targets kept in the volumes cratarr-android-ndk, cargo-android-bin and
# cargo-android-rustup-<version>, so each is fetched once. The bridge's Kotlin stays in the repository, in
# android/app/src/main/java/app/cratarr/rules/, for the unit tests: a library that Kotlin does not fit stops here, the file
# written anew, as the two only work as a pair.
set -eu
cd "$(dirname "$0")/.."
rust=$(tr -d '\r' < Dockerfile | sed -n 's/^FROM rust:\([0-9.]*\)-alpine.* AS source$/\1/p')
ndk=r27c
docker run --rm -v "$(pwd):/src" -v cratarr-android-ndk:/opt/ndk -v cargo-android:/usr/local/cargo/registry \
  -v cargo-android-bin:/opt/cargo-bin -v "cargo-android-rustup-$rust:/usr/local/rustup" -w /src/server/rules-ffi \
  -e CARGO_TARGET_DIR=/tmp/t -e NDK=$ndk "rust:$rust-bookworm" sh -euc '
  if [ ! -d /opt/ndk/android-ndk-$NDK ]; then
    curl -sSfL -o /tmp/ndk.zip https://dl.google.com/android/repository/android-ndk-$NDK-linux.zip
    unzip -q /tmp/ndk.zip -d /opt/ndk && rm /tmp/ndk.zip
  fi
  export ANDROID_NDK_HOME=/opt/ndk/android-ndk-$NDK PATH=/opt/cargo-bin/bin:$PATH
  for target in aarch64-linux-android x86_64-linux-android; do
    [ -d "$(rustc --print sysroot)/lib/rustlib/$target" ] || rustup target add "$target" >/dev/null 2>&1
  done
  command -v cargo-ndk >/dev/null || cargo install -q --locked --root /opt/cargo-bin cargo-ndk
  cargo ndk -t arm64-v8a -t x86_64 -o /src/android/app/build/rustJniLibs build --release
  # The rules crate is a cdylib too (for the page): only the bridge goes in the app.
  rm -f /src/android/app/build/rustJniLibs/*/libcratarr_rules.so
  chown -R "$(stat -c %u:%g /src)" /src/android/app/build/rustJniLibs
  # The Kotlin from the metadata in the bridge, which the release build strips: read from an unstripped host build.
  cargo build -q --lib
  cargo run -q --bin uniffi-bindgen -- generate --library /tmp/t/debug/libcratarr_rules_ffi.so \
    --language kotlin --no-format --out-dir /tmp/kotlin
  made=/tmp/kotlin/app/cratarr/rules/cratarr_rules_ffi.kt
  kept=/src/android/app/src/main/java/app/cratarr/rules/cratarr_rules_ffi.kt
  if ! tr -d "\r" < "$kept" | cmp -s - "$made"; then
    cp "$made" "$kept"
    chown "$(stat -c %u:%g /src)" "$kept"
    echo "The rules bridge changed, so its Kotlin is written anew: android/app/src/main/java/app/cratarr/rules/. Build again." >&2
    exit 1
  fi
'
ls -l android/app/build/rustJniLibs/*/
