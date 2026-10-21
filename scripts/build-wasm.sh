#!/usr/bin/env bash
# Build the browser engine: sysml-wasm for wasm32-unknown-unknown plus its
# wasm-bindgen JS glue (ES module, for Web Workers).
#
#   scripts/build-wasm.sh [out-dir]   (default: target/sysml-wasm)
#
# Needs: rustup target wasm32-unknown-unknown, a clang that targets wasm32
# (for the tree-sitter C sources), and wasm-bindgen-cli matching the
# wasm-bindgen version pinned in crates/sysml-wasm/Cargo.toml.
set -euo pipefail

cd "$(dirname "$0")/.."
out="${1:-target/sysml-wasm}"

export CC_wasm32_unknown_unknown="${CC_wasm32_unknown_unknown:-clang}"
cargo build -p sysml-wasm --target wasm32-unknown-unknown --profile wasm

rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" \
    target/wasm32-unknown-unknown/wasm/sysml_wasm.wasm
cp LICENSE-APACHE LICENSE-MIT "$out/"

wasm="$out/sysml_wasm_bg.wasm"
printf 'sysml-wasm %s\n  raw:  %s bytes\n  gzip: %s bytes\n' \
    "$(cargo pkgid -p sysml-wasm | sed 's/.*[#@]//')" \
    "$(wc -c <"$wasm")" "$(gzip -9 -c "$wasm" | wc -c)"
