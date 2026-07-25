[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runtimeRoot = Join-Path $projectRoot 'vm\runtime'
$pidPath = Join-Path $runtimeRoot 'qemu.pid'

if (-not (Test-Path -LiteralPath $pidPath)) {
    Write-Host 'Atom Arc Ubuntu VM is not running.'
    exit 0
}
$vmPid = [int](Get-Content -Raw -LiteralPath $pidPath)
$process = Get-Process -Id $vmPid -ErrorAction SilentlyContinue
if (-not $process) {
    Remove-Item -LiteralPath $pidPath -Force
    Write-Host 'Removed stale VM PID file.'
    exit 0
}

& (Join-Path $PSScriptRoot 'Connect-AtomArcUbuntuVm.ps1') 'sudo poweroff'
$process.WaitForExit(30000) | Out-Null
if (-not $process.HasExited) {
    throw "VM did not power off within 30 seconds; QEMU PID $vmPid is still running."
}
Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
Write-Host 'Atom Arc Ubuntu VM stopped cleanly.'
