[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$linuxRoot = '/mnt/c/Projects/atom-arc-driver'

& (Join-Path $PSScriptRoot 'Build-AtomArcShaders.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc shader build failed' }

& wsl.exe -d Ubuntu -- bash "$linuxRoot/native/dxcore/build-wsl.sh"
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc DxCore helper build failed' }

$build = "cd $linuxRoot && export CARGO_TARGET_DIR=target/wsl && export RUSTFLAGS='-D warnings' && cargo build --release -p atom-arcctl"
& wsl.exe -d Ubuntu -- bash -lc $build
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc compound-lab release build failed' }

$matrix = @(
    @{ Name = '4k'; Bytes = 4096; Operations = 64; Warmups = 5; Timeout = 5000 },
    @{ Name = '64k'; Bytes = 65536; Operations = 32; Warmups = 5; Timeout = 5000 },
    @{ Name = '1m'; Bytes = 1048576; Operations = 16; Warmups = 5; Timeout = 5000 },
    @{ Name = '16m'; Bytes = 16777216; Operations = 4; Warmups = 3; Timeout = 5000 },
    @{ Name = '256m'; Bytes = 268435456; Operations = 2; Warmups = 1; Timeout = 10000 }
)

$rejected = [System.Collections.Generic.List[string]]::new()
foreach ($case in $matrix) {
    $output = "experiments/results/live-a380-compound-double-buffer-$($case.Name).json"
    $command = "cd $linuxRoot && ./target/wsl/release/atom-arcctl experiment-live-compound --payload-bytes $($case.Bytes) --operations $($case.Operations) --samples 20 --warmups $($case.Warmups) --timeout-ms $($case.Timeout) --output $output"
    & wsl.exe -d Ubuntu -- bash -lc $command
    $status = $LASTEXITCODE
    if ($status -eq 1) {
        $rejected.Add($case.Name)
    } elseif ($status -ne 0) {
        throw "Compound experiment failed to execute at payload $($case.Bytes)"
    }
}

if ($rejected.Count -eq 0) {
    Write-Host 'ATOM_ARC_COMPOUND_MATRIX_RETAINED'
} else {
    Write-Host "ATOM_ARC_COMPOUND_MATRIX_REJECTED=$($rejected -join ',')"
    throw 'Retained double-buffered-preload compound regressed its live gate'
}
