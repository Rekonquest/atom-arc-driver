[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$syncScript = Join-Path $PSScriptRoot 'Sync-AtomArcUbuntuVm.ps1'
$connectScript = Join-Path $PSScriptRoot 'Connect-AtomArcUbuntuVm.ps1'
$keyPath = Join-Path $projectRoot 'vm\runtime\id_ed25519'
$knownHostsPath = Join-Path $projectRoot 'vm\runtime\known_hosts'
$remoteModule = '/home/atomdev/atom-arc-driver/kernel/atom_arc/atom_arc.ko'

& $syncScript
if ($LASTEXITCODE -ne 0) {
    throw "Ubuntu VM source synchronization failed with exit code $LASTEXITCODE"
}

$remoteGate = @'
set -eu
cd /home/atomdev/atom-arc-driver/kernel/atom_arc
make clean
make
make sparse
modinfo ./atom_arc.ko
bash ./check-stage0-policy.sh ./atom_arc.ko
sudo rmmod atom_arc 2>/dev/null || true
default_log_start=$(($(sudo dmesg | wc -l) + 1))
sudo insmod ./atom_arc.ko
default_logs=$(mktemp)
sudo dmesg | tail -n +"${default_log_start}" >"${default_logs}"
if grep -Eiq 'firmware|GuC|HuC|GSC' "${default_logs}"; then
  echo 'Stage 0 module load unexpectedly touched a firmware path.' >&2
  rm -f "${default_logs}"
  exit 1
fi
rm -f "${default_logs}"
sudo rmmod atom_arc
echo 'ATOM_ARC_STAGE0_NO_FIRMWARE_PATH=PASS'
sha256sum atom_arc.ko
'@

& $connectScript $remoteGate
if ($LASTEXITCODE -ne 0) {
    throw "Atom Arc kernel-module gate failed with exit code $LASTEXITCODE"
}

$sshArguments = @(
    '-p', '2222',
    '-i', $keyPath,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new',
    '-o', ('UserKnownHostsFile=' + $knownHostsPath),
    'atomdev@127.0.0.1'
)
$kernelRelease = (& ssh.exe @sshArguments 'uname -r').Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($kernelRelease)) {
    throw 'Could not determine the tested Ubuntu VM kernel release'
}

$artifactDirectory = Join-Path $projectRoot 'target\kernel'
$artifactPath = Join-Path $artifactDirectory "atom_arc-$kernelRelease.ko"
New-Item -ItemType Directory -Path $artifactDirectory -Force | Out-Null
$scpArguments = @(
    '-P', '2222',
    '-i', $keyPath,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new',
    '-o', ('UserKnownHostsFile=' + $knownHostsPath),
    ('atomdev@127.0.0.1:' + $remoteModule),
    $artifactPath
)
& scp.exe @scpArguments
if ($LASTEXITCODE -ne 0) {
    throw "Could not copy the tested Atom Arc module with exit code $LASTEXITCODE"
}

$remoteHash = ((& ssh.exe @sshArguments "sha256sum '$remoteModule'") -split '\s+')[0].ToLowerInvariant()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($remoteHash)) {
    throw 'Could not hash the tested Atom Arc module in the Ubuntu VM'
}
$localHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifactPath).Hash.ToLowerInvariant()
if ($localHash -ne $remoteHash) {
    throw "Tested kernel-module artifact hash mismatch: remote=$remoteHash local=$localHash"
}

Write-Host "ATOM_ARC_KERNEL_ARTIFACT=$artifactPath"
Write-Host "ATOM_ARC_KERNEL_SHA256=$localHash"
Write-Host 'Atom Arc Stage 0 kernel build, Sparse, metadata, no-firmware-path, load, and unload gates passed.'
