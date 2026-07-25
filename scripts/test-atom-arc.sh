#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/linux}"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p atom-arcctl

if [[ -e /dev/dxg && -x /usr/lib/wsl/lib/libdxcore.so && -f /usr/include/directx/dxcore.h ]]; then
    "$root/native/dxcore/build-wsl.sh" "$CARGO_TARGET_DIR/release/atom-arc-dxcore-helper"
fi

"$CARGO_TARGET_DIR/release/atom-arcctl" probe
"$CARGO_TARGET_DIR/release/atom-arcctl" security-audit
"$CARGO_TARGET_DIR/release/atom-arcctl" self-test
if [[ -x "$CARGO_TARGET_DIR/release/atom-arc-dxcore-helper" ]]; then
    "$CARGO_TARGET_DIR/release/atom-arcctl" hardware-smoke --timeout-ms 5000
    "$CARGO_TARGET_DIR/release/atom-arcctl" doctor --require-hardware
else
    "$CARGO_TARGET_DIR/release/atom-arcctl" doctor
fi

echo ATOM_ARC_LINUX_READY
