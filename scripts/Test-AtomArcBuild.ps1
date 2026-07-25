[CmdletBinding()]
param(
    [switch]$SkipDocker,
    [switch]$SkipWsl,
    [switch]$SkipKernel,
    [switch]$SkipLivePrimitives
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root 'Cargo.toml'
$binary = Join-Path $root 'target\release\atom-arcctl.exe'

& cargo fmt --manifest-path $manifest --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'cargo fmt failed' }

& cargo clippy --manifest-path $manifest --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'cargo clippy failed' }

& cargo test --manifest-path $manifest --workspace
if ($LASTEXITCODE -ne 0) { throw 'cargo test failed' }

& cargo build --manifest-path $manifest --release -p atom-arcctl
if ($LASTEXITCODE -ne 0) { throw 'release build failed' }

& $binary probe
if ($LASTEXITCODE -ne 0) { throw 'host probe failed' }
& $binary security-audit
if ($LASTEXITCODE -ne 0) { throw 'security audit failed' }
& $binary self-test
if ($LASTEXITCODE -ne 0) { throw 'self-test failed' }
& $binary doctor
if ($LASTEXITCODE -ne 0) { throw 'doctor failed' }

$crateRoots = Get-ChildItem -LiteralPath (Join-Path $root 'crates') -Directory
foreach ($crateRoot in $crateRoots) {
    $lineCount = 0
    Get-ChildItem -LiteralPath $crateRoot.FullName -Recurse -Filter '*.rs' | ForEach-Object {
        $lineCount += (Get-Content -LiteralPath $_.FullName).Count
    }
    if ($lineCount -ge 4000) {
        throw "Crate $($crateRoot.Name) has $lineCount Rust source lines; the limit is below 4000"
    }
    Write-Host "crate-lines $($crateRoot.Name)=$lineCount"
}

& (Join-Path $PSScriptRoot 'Test-AtomArcExperimentLab.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Experiment significance gate failed' }

if (-not $SkipKernel) {
    & (Join-Path $PSScriptRoot 'Test-AtomArcKernelModule.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Independent kernel-module gate failed' }
}

if (-not $SkipWsl) {
    & (Join-Path $PSScriptRoot 'Test-AtomArcWsl.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'WSL verification failed' }

    if (-not $SkipLivePrimitives) {
        & (Join-Path $PSScriptRoot 'Test-AtomArcLivePrimitives.ps1')
        if ($LASTEXITCODE -ne 0) { throw 'Live A380 primitive matrix failed' }
        & (Join-Path $PSScriptRoot 'Test-AtomArcCompoundPrimitives.ps1')
        if ($LASTEXITCODE -ne 0) { throw 'Live A380 compound matrix failed' }
    }
}

if (-not $SkipDocker) {
    & (Join-Path $PSScriptRoot 'Test-AtomArcDocker.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Docker verification failed' }
}

Write-Host 'ATOM_ARC_BUILD_READY'
