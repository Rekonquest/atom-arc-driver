[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root 'Cargo.toml'
$binary = Join-Path $root 'target\release\atom-arcctl.exe'
$statisticsFixture = Join-Path $root 'experiments\fixtures\promote-pinned-lanes.json'
$reject = Join-Path $root 'experiments\fixtures\reject-corrupt-output.json'
$invalidStack = Join-Path $root 'experiments\fixtures\invalid-reordered-stack.json'

& cargo test --manifest-path $manifest -p atom-arc-lab
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc experiment unit tests failed' }
& cargo build --manifest-path $manifest --release -p atom-arcctl
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc control-plane release build failed' }

$fixtureJson = & $binary experiment-evaluate --input $statisticsFixture --json
if ($LASTEXITCODE -ne 0) { throw 'Statistics-only experiment fixture was not accepted' }
$fixtureReport = $fixtureJson | ConvertFrom-Json
if ($fixtureReport.verdict -ne 'fixture-valid') {
    throw 'Synthetic fixture escaped the fixture-only evidence boundary'
}
& $binary experiment-evaluate --input $reject
if ($LASTEXITCODE -ne 1) { throw 'Corrupt-output experiment fixture was not rejected' }
& $binary experiment-evaluate --input $invalidStack
if ($LASTEXITCODE -ne 2) { throw 'Reordered primitive stack was not rejected as invalid' }
& $binary --version | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc control-plane binary did not reset cleanly' }

Write-Host 'ATOM_ARC_EXPERIMENT_LAB_READY'
