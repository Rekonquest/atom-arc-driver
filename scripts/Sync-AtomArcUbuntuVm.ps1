[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runtimeRoot = Join-Path $projectRoot 'vm\runtime'
$archivePath = Join-Path $runtimeRoot 'atom-arc-driver-source.tar.gz'
$keyPath = Join-Path $runtimeRoot 'id_ed25519'
$knownHostsPath = Join-Path $runtimeRoot 'known_hosts'

if (-not (Test-Path -LiteralPath $keyPath)) {
    throw 'VM SSH key is missing. Run .\scripts\New-AtomArcUbuntuVm.ps1 first.'
}

Remove-Item -LiteralPath $archivePath -Force -ErrorAction SilentlyContinue
& tar.exe -czf $archivePath `
    --exclude='./.git' `
    --exclude='./target' `
    --exclude='./vm/runtime' `
    -C $projectRoot .
if ($LASTEXITCODE -ne 0) {
    throw "Source archive creation failed with exit code $LASTEXITCODE"
}

$scpArguments = @(
    '-P', '2222',
    '-i', $keyPath,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new',
    '-o', ('UserKnownHostsFile=' + $knownHostsPath),
    $archivePath,
    'atomdev@127.0.0.1:/tmp/atom-arc-driver-source.tar.gz'
)
& scp.exe @scpArguments
if ($LASTEXITCODE -ne 0) {
    throw "Source upload failed with exit code $LASTEXITCODE"
}

& (Join-Path $PSScriptRoot 'Connect-AtomArcUbuntuVm.ps1') `
    'mkdir -p /home/atomdev/atom-arc-driver && tar -xzf /tmp/atom-arc-driver-source.tar.gz -C /home/atomdev/atom-arc-driver && rm /tmp/atom-arc-driver-source.tar.gz'
if ($LASTEXITCODE -ne 0) {
    throw "Source extraction failed with exit code $LASTEXITCODE"
}

Write-Host 'Atom Arc source synchronized to /home/atomdev/atom-arc-driver'
