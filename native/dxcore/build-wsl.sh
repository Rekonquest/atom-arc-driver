#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output="${1:-$root/target/wsl/release/atom-arc-dxcore-helper}"
generated="$root/target/wsl/generated"
shader_header="$generated/atom_arc_compute_dxil.h"
if [[ ! -s "$shader_header" ]]; then
    echo "Missing generated shader header: $shader_header" >&2
    echo "Run scripts/Build-AtomArcShaders.ps1 from Windows first." >&2
    exit 1
fi
mkdir -p "$(dirname "$output")"

read -r -a directx_cflags <<<"$(pkg-config --cflags DirectX-Headers)"
read -r -a directx_libs <<<"$(pkg-config --libs DirectX-Headers)"

g++ \
    -std=c++20 \
    -O2 \
    -Wall \
    -Wextra \
    -Werror \
    "${directx_cflags[@]}" \
    -I"$generated" \
    "$root/native/dxcore/atom_arc_dxcore.cpp" \
    -L/usr/lib/wsl/lib \
    -Wl,-rpath,/usr/lib/wsl/lib \
    -ldxcore \
    -ld3d12 \
    "${directx_libs[@]}" \
    -pthread \
    -o "$output"

echo "DXCORE_HELPER_BUILT=$output"
