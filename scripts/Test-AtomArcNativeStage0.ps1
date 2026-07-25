[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9.-]+$')]
    [string]$NativeGuestHost,

    [ValidateRange(1, 65535)]
    [int]$Port = 22,

    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$User = 'atomdev',

    [string]$IdentityFile,

    [ValidatePattern('^/[A-Za-z0-9._/-]+$')]
    [string]$RemoteProjectRoot = '/home/atomdev/atom-arc-driver'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$parsedAddress = $null
$isIpAddress = [Net.IPAddress]::TryParse($NativeGuestHost, [ref]$parsedAddress)
if (
    $NativeGuestHost -in @('localhost', 'localhost.localdomain') -or
    ($isIpAddress -and [Net.IPAddress]::IsLoopback($parsedAddress))
) {
    throw 'Native Stage 0 refuses loopback: the Windows WHPX VM has no passed-through A380.'
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $projectRoot 'experiments\results\atom-arc-native-stage0-latest.json'
$sshOptions = @(
    '-p', $Port,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new'
)
$scpOptions = @(
    '-P', $Port,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new'
)
if (-not [string]::IsNullOrWhiteSpace($IdentityFile)) {
    $resolvedIdentity = (Resolve-Path -LiteralPath $IdentityFile).Path
    $sshOptions += @('-i', $resolvedIdentity)
    $scpOptions += @('-i', $resolvedIdentity)
}

$remote = "${User}@${NativeGuestHost}"
$remoteCommand = "cd '$RemoteProjectRoot' && sudo -n bash linux-guest/03-test-native-stage0.sh"
$startedAt = [DateTimeOffset]::UtcNow
& ssh.exe @sshOptions $remote $remoteCommand
if ($LASTEXITCODE -ne 0) {
    throw 'Atom Arc native Stage 0 gate failed. No passing evidence was accepted.'
}

$remoteEvidence = "${remote}:${RemoteProjectRoot}/experiments/results/atom-arc-native-stage0-latest.json"
& scp.exe @scpOptions $remoteEvidence $destination
if ($LASTEXITCODE -ne 0) {
    throw 'Could not copy the native Stage 0 evidence artifact.'
}

$evidence = Get-Content -LiteralPath $destination -Raw | ConvertFrom-Json
$testedAt = [DateTimeOffset]::Parse($evidence.tested_at_utc)
$localHashes = @{
    'atom_arc_main.c' = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $projectRoot 'kernel\atom_arc\atom_arc_main.c')).Hash.ToLowerInvariant()
    'Makefile' = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $projectRoot 'kernel\atom_arc\Makefile')).Hash.ToLowerInvariant()
    'check-stage0-policy.sh' = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $projectRoot 'kernel\atom_arc\check-stage0-policy.sh')).Hash.ToLowerInvariant()
}
$reportedHashes = $evidence.build.source_hashes
$sourceHashesMatch =
    $reportedHashes.'atom_arc_main.c' -eq $localHashes.'atom_arc_main.c' -and
    $reportedHashes.Makefile -eq $localHashes.Makefile -and
    $reportedHashes.'check-stage0-policy.sh' -eq $localHashes.'check-stage0-policy.sh'
$stage1 = $evidence.stage1_activity_absence

if (
    $evidence.schema -ne 1 -or
    $evidence.result -ne 'passed' -or
    -not $evidence.native_path -or
    $evidence.stage -ne 0 -or
    $evidence.driver -ne 'atom_arc' -or
    [string]::IsNullOrWhiteSpace($evidence.run_id) -or
    $testedAt -lt $startedAt.AddMinutes(-5) -or
    $testedAt -gt [DateTimeOffset]::UtcNow.AddMinutes(5) -or
    $evidence.device.gpu_bdf -notmatch '^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$' -or
    $evidence.device.audio_bdf -notmatch '^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$' -or
    $evidence.device.pci_id -ne '8086:56a5' -or
    $evidence.device.audio_pci_id -ne '8086:4f92' -or
    $evidence.device.subsystem_id -ne '172f:3943' -or
    $evidence.device.revision -ne '0x05' -or
    $evidence.ownership.probe_cycles -ne 2 -or
    $evidence.ownership.clean_unloads -ne 2 -or
    $evidence.ownership.dma_address_bits -ne 46 -or
    -not $evidence.ownership.regions_claimed -or
    -not $evidence.ownership.mmio_mapped -or
    $evidence.ownership.mmio_read_count -ne 0 -or
    $evidence.ownership.bus_master_enabled -or
    $evidence.ownership.bar0_bytes -le 0 -or
    $evidence.ownership.bar2_bytes -le 0 -or
    $evidence.ownership.bar0_start -notmatch '^0x[1-9a-f][0-9a-f]*$' -or
    $evidence.ownership.bar2_start -notmatch '^0x[1-9a-f][0-9a-f]*$' -or
    -not $evidence.build.source_policy_passed -or
    [string]::IsNullOrWhiteSpace($evidence.build.kernel_release) -or
    $evidence.build.module_sha256 -notmatch '^[0-9a-f]{64}$' -or
    -not $sourceHashesMatch -or
    $stage1.firmware_started -or
    $stage1.irq_allocated -or
    $stage1.dma_transfer_attempted -or
    $stage1.reset_attempted -or
    $stage1.submission_attempted
) {
    throw 'Copied native Stage 0 evidence failed strict host-side validation.'
}

Write-Host "ATOM_ARC_NATIVE_STAGE0_RUN_ID=$($evidence.run_id)"
Write-Host 'Atom Arc native Stage 0 ownership, ordered resource setup, zero-MMIO, and clean-unload gate passed.'
