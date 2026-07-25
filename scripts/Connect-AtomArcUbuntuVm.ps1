[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Command
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$keyPath = Join-Path $projectRoot 'vm\runtime\id_ed25519'
if (-not (Test-Path -LiteralPath $keyPath)) {
    throw 'VM SSH key is missing. Run .\scripts\New-AtomArcUbuntuVm.ps1 first.'
}

$sshArguments = @(
    '-p', '2222',
    '-i', $keyPath,
    '-o', 'IdentitiesOnly=yes',
    '-o', 'StrictHostKeyChecking=accept-new',
    '-o', ('UserKnownHostsFile=' + (Join-Path $projectRoot 'vm\runtime\known_hosts')),
    'atomdev@127.0.0.1'
)
if ($Command.Count -gt 0) {
    $sshArguments += ($Command -join ' ')
}
& ssh.exe @sshArguments
exit $LASTEXITCODE
