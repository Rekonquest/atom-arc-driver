[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$connectScript = Join-Path $PSScriptRoot 'Connect-AtomArcUbuntuVm.ps1'
$remoteGate = @'
set -eu
cd /home/atomdev/atom-arc-driver
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
'@

& $connectScript $remoteGate
if ($LASTEXITCODE -ne 0) {
    throw "Atom Arc Ubuntu VM gate failed with exit code $LASTEXITCODE"
}
Write-Host 'Atom Arc Ubuntu VM format, test, and Clippy gates passed.'
