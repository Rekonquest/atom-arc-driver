[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    throw 'This repair must run from an elevated PowerShell process.'
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$logRoot = Join-Path $projectRoot 'vm\runtime'
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
$logPath = Join-Path $logRoot 'chkdsk-D-before-linux.log'
$expectedSerialFragment = '4A24_A769'
$expectedPartitionGuid = [guid]'96eb4609-1452-4360-ba54-079006c07a65'
$repairSucceeded = $false

Start-Transcript -LiteralPath $logPath -Force | Out-Null
try {
    & pnputil.exe /scan-devices
    Update-HostStorageCache
    Start-Sleep -Seconds 2

    $disk = Get-Disk | Where-Object {
        $_.FriendlyName -eq 'WD_BLACK SN770 1TB' -and
        $_.SerialNumber -match $expectedSerialFragment
    }
    if (@($disk).Count -ne 1) {
        throw 'The expected 1 TB SN770 did not return to the Windows storage stack after a PnP rescan. Reboot is required.'
    }
    if ($disk.IsBoot -or $disk.IsSystem) {
        throw 'Safety check failed: the target unexpectedly reports itself as a boot or system disk.'
    }

    $partition = Get-Partition -DiskNumber $disk.Number | Where-Object {
        [guid]$_.Guid -eq $expectedPartitionGuid
    }
    if (@($partition).Count -ne 1) {
        throw "Safety check failed: expected NTFS partition $expectedPartitionGuid was not found on the target disk."
    }

    $volumePath = "\\?\Volume{$expectedPartitionGuid}\"
    Write-Host "Repairing $volumePath on disk $($disk.Number) '$($disk.FriendlyName)'"
    Write-Host 'The volume will be dismounted temporarily.'
    & chkdsk.exe $volumePath /F /X
    $chkdskExit = $LASTEXITCODE
    Write-Host "chkdsk exit code: $chkdskExit"
    if ($chkdskExit -notin 0, 1, 2) {
        throw "chkdsk failed with exit code $chkdskExit"
    }
    $repairSucceeded = $true
} finally {
    Stop-Transcript | Out-Null
}

if ($repairSucceeded) {
    exit 0
}
