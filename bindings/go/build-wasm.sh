#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

rustup target add wasm32-unknown-unknown
cargo build \
  --manifest-path "${ROOT_DIR}/Cargo.toml" \
  --release \
  --target wasm32-unknown-unknown

cp \
  "${ROOT_DIR}/target/wasm32-unknown-unknown/release/compiler_rs.wasm" \
  "${ROOT_DIR}/bindings/go/compiler_rs.wasm"

cp \
  "${ROOT_DIR}/target/wasm32-unknown-unknown/release/compiler_rs.wasm" \
  "${ROOT_DIR}/bindings/go/internal/assets/compiler_rs.wasm"

echo "WASM written to ${ROOT_DIR}/bindings/go/compiler_rs.wasm"
