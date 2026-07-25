[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$linuxRoot = '/mnt/c/Projects/atom-arc-driver'

& (Join-Path $PSScriptRoot 'Build-AtomArcShaders.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc shader build failed' }

& wsl.exe -d Ubuntu -- test -f /usr/include/directx/dxcore.h
if ($LASTEXITCODE -ne 0) {
    & wsl.exe -d Ubuntu -u root -- apt-get update
    if ($LASTEXITCODE -ne 0) { throw 'Could not update Ubuntu package metadata' }
    & wsl.exe -d Ubuntu -u root -- apt-get install -y --no-install-recommends directx-headers-dev g++ pkg-config
    if ($LASTEXITCODE -ne 0) { throw 'Could not install WSL DxCore build dependencies' }
}

& wsl.exe -d Ubuntu -- bash "$linuxRoot/native/dxcore/build-wsl.sh"
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc DxCore helper build failed' }
& wsl.exe -d Ubuntu -- bash "$linuxRoot/native/dxcore/test-wsl.sh"
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc DxCore native tests failed' }

$command = "cd $linuxRoot && export CARGO_TARGET_DIR=target/wsl && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo build --release -p atom-arcctl && ./target/wsl/release/atom-arcctl probe && ./target/wsl/release/atom-arcctl security-audit && ./target/wsl/release/atom-arcctl self-test && ./target/wsl/release/atom-arcctl hardware-smoke --timeout-ms 5000 && ./target/wsl/release/atom-arcctl compute --groups 16 --addend 7 --timeout-ms 5000 && ./target/wsl/release/atom-arcctl stress --groups 16 --iterations 3 --timeout-ms 5000 && ./target/wsl/release/atom-arcctl doctor --require-hardware"

& wsl.exe -d Ubuntu -- bash -lc $command
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc WSL gate failed' }
Write-Host 'ATOM_ARC_WSL_READY'
