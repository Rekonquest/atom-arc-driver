[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArguments
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$linuxBinary = '/mnt/c/Projects/atom-arc-driver/target/wsl/release/atom-arcctl'
& wsl.exe -d Ubuntu -- test -x $linuxBinary
if ($LASTEXITCODE -ne 0) {
    throw 'The Atom Arc WSL release is missing. Run scripts\Test-AtomArcWsl.ps1 first.'
}

if ($CommandArguments.Count -eq 0) {
    $CommandArguments = @('doctor', '--require-hardware')
}

& wsl.exe -d Ubuntu -- $linuxBinary @CommandArguments
exit $LASTEXITCODE
