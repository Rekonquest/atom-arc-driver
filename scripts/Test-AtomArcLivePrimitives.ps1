[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path -Parent $PSScriptRoot
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

$build = "cd $linuxRoot && export CARGO_TARGET_DIR=target/wsl && export RUSTFLAGS='-D warnings' && cargo build --release -p atom-arcctl"
& wsl.exe -d Ubuntu -- bash -lc $build
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc live-lab release build failed' }

$matrix = @(
    @{ Name = '4k'; Bytes = 4096; Operations = 64; Warmups = 5; Timeout = 5000 },
    @{ Name = '64k'; Bytes = 65536; Operations = 32; Warmups = 5; Timeout = 5000 },
    @{ Name = '1m'; Bytes = 1048576; Operations = 16; Warmups = 5; Timeout = 5000 },
    @{ Name = '16m'; Bytes = 16777216; Operations = 4; Warmups = 3; Timeout = 5000 },
    @{ Name = '256m'; Bytes = 268435456; Operations = 1; Warmups = 1; Timeout = 10000 }
)

foreach ($case in $matrix) {
    $output = "experiments/results/live-a380-reusable-staging-$($case.Name).json"
    $command = "cd $linuxRoot && ./target/wsl/release/atom-arcctl experiment-live-transfer --payload-bytes $($case.Bytes) --operations $($case.Operations) --samples 20 --warmups $($case.Warmups) --timeout-ms $($case.Timeout) --output $output"
    & wsl.exe -d Ubuntu -- bash -lc $command
    if ($LASTEXITCODE -ne 0) {
        throw "Reusable staging pool did not retain at payload $($case.Bytes)"
    }
}

Write-Host 'ATOM_ARC_LIVE_PRIMITIVE_MATRIX_READY'
