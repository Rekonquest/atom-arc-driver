[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    throw 'This partition resize must run from an elevated PowerShell process.'
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$logRoot = Join-Path $projectRoot 'vm\runtime'
New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
$logPath = Join-Path $logRoot 'reserve-E-for-atom-linux.log'
$expectedSerialFragment = '3851_5140_3BE1'
$expectedPartitionGuid = [guid]'0e457a10-0ee3-4582-a75d-f8664e1f6758'
$reservationBytes = 200GB
$succeeded = $false

Start-Transcript -LiteralPath $logPath -Force | Out-Null
try {
    Update-HostStorageCache
    $disk = Get-Disk | Where-Object {
        $_.FriendlyName -eq 'Samsung SSD 990 EVO Plus 1TB' -and
        $_.SerialNumber -match $expectedSerialFragment
    }
    if (@($disk).Count -ne 1) {
        throw 'Safety check failed: the expected 1 TB Samsung target was not uniquely identified.'
    }
    if ($disk.IsBoot -or $disk.IsSystem -or $disk.IsOffline -or $disk.IsReadOnly) {
        throw 'Safety check failed: the target disk is boot/system, offline, or read-only.'
    }

    $partition = Get-Partition -DiskNumber $disk.Number | Where-Object {
        [guid]$_.Guid -eq $expectedPartitionGuid
    }
    if (@($partition).Count -ne 1) {
        throw "Safety check failed: expected E: partition $expectedPartitionGuid was not found."
    }
    if ($partition.Type -ne 'Basic') {
        throw "Safety check failed: expected a Basic data partition, got '$($partition.Type)'."
    }

    $targetSize = $partition.Size - $reservationBytes
    $supported = Get-PartitionSupportedSize `
        -DiskNumber $disk.Number `
        -PartitionNumber $partition.PartitionNumber
    if ($targetSize -lt $supported.SizeMin -or $targetSize -gt $supported.SizeMax) {
        throw "NTFS cannot shrink safely to $([math]::Round($targetSize / 1GB, 2)) GiB; supported range is $([math]::Round($supported.SizeMin / 1GB, 2))-$([math]::Round($supported.SizeMax / 1GB, 2)) GiB."
    }

    Write-Host "Disk: $($disk.Number) $($disk.FriendlyName) $($disk.SerialNumber)"
    Write-Host "Partition: $($partition.PartitionNumber) $($partition.Guid)"
    Write-Host "Current size: $([math]::Round($partition.Size / 1GB, 2)) GiB"
    Write-Host "New size: $([math]::Round($targetSize / 1GB, 2)) GiB"
    Write-Host 'Creating exactly 200 GiB of unallocated space without formatting it.'

    Resize-Partition `
        -DiskNumber $disk.Number `
        -PartitionNumber $partition.PartitionNumber `
        -Size $targetSize

    Update-HostStorageCache
    $verifiedDisk = Get-Disk -Number $disk.Number
    $verifiedPartition = Get-Partition `
        -DiskNumber $disk.Number `
        -PartitionNumber $partition.PartitionNumber
    if ([math]::Abs($verifiedPartition.Size - $targetSize) -gt 1MB) {
        throw 'Post-resize partition-size verification failed.'
    }
    if ($verifiedDisk.LargestFreeExtent -lt (199GB)) {
        throw 'Post-resize free-extent verification failed.'
    }

    Write-Host "Verified unallocated extent: $([math]::Round($verifiedDisk.LargestFreeExtent / 1GB, 2)) GiB"
    $succeeded = $true
} catch {
    Write-Error $_
    throw
} finally {
    Stop-Transcript | Out-Null
}

if ($succeeded) {
    exit 0
}
