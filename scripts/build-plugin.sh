#!/usr/bin/env bash
# Build the Rust/SWC plugin as a WASI artifact, suitable for @swc/core's
# wasm plugin host.
#
# Why this wrapper exists: this dev environment exports a global RUSTFLAGS
# (`-C lto=thin ...`) that breaks proc-macro crate builds with:
#   error: lto cannot be used for `proc-macro` crate type without `-Zdylib-lto`
#
# Cargo's flag-source precedence puts the RUSTFLAGS env var ABOVE every
# config-file mechanism (`build.rustflags`, `target.<triple>.rustflags`,
# `[env]` table), so we MUST unset it in the calling shell.
#
# Use this wrapper for all cargo commands in this repo until the env var
# is fixed upstream:
#   ./scripts/build-plugin.sh check
#   ./scripts/build-plugin.sh build --release
#   ./scripts/build-plugin.sh test
#   ./scripts/build-plugin.sh wasm     # special: builds the WASI plugin

set -euo pipefail
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS

cmd="${1:-check}"
shift || true

case "$cmd" in
  wasm)
    rustup target add wasm32-wasip1 >/dev/null 2>&1 || true
    exec cargo build --release \
      --target wasm32-wasip1 \
      -p swc-plugin-formatjs \
      --features plugin \
      --no-default-features \
      "$@"
    ;;
  *)
    exec cargo "$cmd" -p swc-plugin-formatjs "$@"
    ;;
esac
