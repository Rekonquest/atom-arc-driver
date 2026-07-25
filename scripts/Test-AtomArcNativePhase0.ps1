[CmdletBinding()]
param(
    [ValidateRange(1000, 1000000)]
    [int]$HeartbeatCycles = 1000
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
throw 'Native Phase 0 is intentionally disabled while the ordered Stage 0 hardware gate is incomplete.'
$projectRoot = Split-Path -Parent $PSScriptRoot
$syncScript = Join-Path $PSScriptRoot 'Sync-AtomArcUbuntuVm.ps1'
$connectScript = Join-Path $PSScriptRoot 'Connect-AtomArcUbuntuVm.ps1'
$runtimeRoot = Join-Path $projectRoot 'vm\runtime'
$keyPath = Join-Path $runtimeRoot 'id_ed25519'
$knownHostsPath = Join-Path $runtimeRoot 'known_hosts'

& $syncScript
if ($LASTEXITCODE -ne 0) {
    throw "Ubuntu VM source synchronization failed with exit code $LASTEXITCODE"
}

$remoteCommand = "cd /home/atomdev/atom-arc-driver && sudo -n bash linux-guest/04-test-native-phase0.sh --cycles $HeartbeatCycles"
& $connectScript $remoteCommand
if ($LASTEXITCODE -ne 0) {
    throw 'Atom Arc native Phase 0 gate failed. This phase remains incomplete.'
}

$sshOptions = @(
    '-i', $keyPath,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new',
    '-o', ('UserKnownHostsFile=' + $knownHostsPath)
)
$copies = @(
    @('atomdev@127.0.0.1:/home/atomdev/atom-arc-driver/atom-arc-native-live-proof.json', (Join-Path $projectRoot 'atom-arc-native-live-proof.json')),
    @('atomdev@127.0.0.1:/home/atomdev/atom-arc-driver/experiments/results/atom-arc-native-phase0-latest.json', (Join-Path $projectRoot 'experiments\results\atom-arc-native-phase0-latest.json'))
)
foreach ($copy in $copies) {
    & scp.exe -P 2222 @sshOptions $copy[0] $copy[1]
    if ($LASTEXITCODE -ne 0) {
        throw "Could not copy native proof artifact $($copy[0])"
    }
}

$proof = Get-Content -LiteralPath (Join-Path $projectRoot 'atom-arc-native-live-proof.json') -Raw | ConvertFrom-Json
$evidence = Get-Content -LiteralPath (Join-Path $projectRoot 'experiments\results\atom-arc-native-phase0-latest.json') -Raw | ConvertFrom-Json
if ($proof.schema -ne 1 -or $proof.path_kind -ne 'atom-native' -or $proof.phase -ne 0) {
    throw 'Copied native proof failed host-side identity validation.'
}
if ($evidence.result -ne 'passed' -or -not $evidence.native_path -or [string]::IsNullOrWhiteSpace($evidence.run_id)) {
    throw 'Copied native evidence failed host-side result validation.'
}

Write-Host "ATOM_ARC_NATIVE_RUN_ID=$($evidence.run_id)"
Write-Host 'Atom Arc native Phase 0 ownership, security, heartbeat, interrupt, reset-recovery, and correctness gate passed.'
