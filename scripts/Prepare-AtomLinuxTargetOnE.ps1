[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    throw 'Target preparation must run from an elevated PowerShell process.'
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$logPath = Join-Path $projectRoot 'vm\runtime\prepare-ubuntu-target-on-E.log'
$expectedSerialFragment = '3851_5140_3BE1'
$expectedDataGuid = [guid]'0e457a10-0ee3-4582-a75d-f8664e1f6758'
$expectedInstallerGuid = [guid]'33d947e4-0ffe-403d-b36d-c8361d724c09'
$efiType = '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}'
$targetEfiSize = 1GB
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

    $partitions = @(Get-Partition -DiskNumber $disk.Number)
    $dataPartition = $partitions | Where-Object { [guid]$_.Guid -eq $expectedDataGuid }
    $installerPartition = $partitions | Where-Object { [guid]$_.Guid -eq $expectedInstallerGuid }
    if (@($dataPartition).Count -ne 1 -or $dataPartition.DriveLetter -ne 'E') {
        throw 'Safety check failed: the preserved E: data partition was not found at its expected identity.'
    }
    if (@($installerPartition).Count -ne 1 -or $installerPartition.GptType -ne $efiType) {
        throw 'Safety check failed: the verified Ubuntu installer ESP was not found.'
    }
    if ($installerPartition.Size -ne 8GB) {
        throw 'Safety check failed: the Ubuntu installer ESP is not exactly 8 GiB.'
    }
    if (-not $installerPartition.DriveLetter) {
        throw 'The Ubuntu installer ESP has no Windows drive letter; cannot verify its boot payload.'
    }
    $installerRoot = "$($installerPartition.DriveLetter):\"
    if (-not (Test-Path -LiteralPath (Join-Path $installerRoot 'EFI\BOOT\BOOTX64.EFI'))) {
        throw 'Safety check failed: the installer ESP is missing EFI\BOOT\BOOTX64.EFI.'
    }
    if ($partitions.Count -ne 3) {
        throw 'Safety check failed: the expected three-partition pre-install layout has changed.'
    }
    if ($disk.LargestFreeExtent -ne 192GB) {
        throw "Expected exactly 192 GiB unallocated; found $([math]::Round($disk.LargestFreeExtent / 1GB, 2)) GiB."
    }

    $dataIdentity = [ordered]@{
        Guid = $dataPartition.Guid
        Offset = $dataPartition.Offset
        Size = $dataPartition.Size
    }
    $installerEnd = $installerPartition.Offset + $installerPartition.Size
    $targetEfi = New-Partition `
        -DiskNumber $disk.Number `
        -Size $targetEfiSize `
        -AssignDriveLetter
    if ($targetEfi.Offset -lt $installerEnd) {
        throw 'Safety check failed: the Ubuntu target ESP overlaps an existing partition.'
    }

    $targetVolume = Format-Volume `
        -Partition $targetEfi `
        -FileSystem FAT32 `
        -NewFileSystemLabel 'ATOMLINUX_EFI' `
        -AllocationUnitSize 4096 `
        -Force `
        -Confirm:$false
    if (-not $targetVolume.DriveLetter) {
        throw 'Formatted Ubuntu target ESP did not retain a drive letter.'
    }
    Set-Partition `
        -DiskNumber $disk.Number `
        -PartitionNumber $targetEfi.PartitionNumber `
        -GptType $efiType
    Update-HostStorageCache

    $mountedTarget = Get-Partition `
        -DiskNumber $disk.Number `
        -PartitionNumber $targetEfi.PartitionNumber
    if ($mountedTarget.DriveLetter) {
        Remove-PartitionAccessPath `
            -DiskNumber $disk.Number `
            -PartitionNumber $targetEfi.PartitionNumber `
            -AccessPath "$($mountedTarget.DriveLetter):\"
    }
    Update-HostStorageCache

    $verifiedDisk = Get-Disk -Number $disk.Number
    $verifiedData = Get-Partition -DiskNumber $disk.Number | Where-Object {
        [guid]$_.Guid -eq $expectedDataGuid
    }
    $verifiedTarget = Get-Partition -DiskNumber $disk.Number -PartitionNumber $targetEfi.PartitionNumber
    if ($verifiedData.Guid -ne $dataIdentity.Guid -or
        $verifiedData.Offset -ne $dataIdentity.Offset -or
        $verifiedData.Size -ne $dataIdentity.Size -or
        $verifiedData.DriveLetter -ne 'E') {
        throw 'Postcondition failed: the preserved E: partition identity or extent changed.'
    }
    if ($verifiedTarget.Size -ne 1GB -or $verifiedTarget.GptType -ne $efiType) {
        throw 'Postcondition failed: the Ubuntu target ESP is not the expected 1 GiB EFI partition.'
    }
    if ($verifiedTarget.DriveLetter) {
        throw 'Postcondition failed: the Ubuntu target ESP is still mounted in Windows.'
    }
    if ($verifiedDisk.LargestFreeExtent -ne 191GB) {
        throw "Postcondition failed: expected exactly 191 GiB for Ubuntu root; found $([math]::Round($verifiedDisk.LargestFreeExtent / 1GB, 2)) GiB."
    }

    Write-Host "Ubuntu target ESP: partition $($verifiedTarget.PartitionNumber), $([math]::Round($verifiedTarget.Size / 1GB, 2)) GiB"
    Write-Host 'Preserved Windows E: partition: unchanged'
    Write-Host "Remaining unallocated extent for Ubuntu root: $([math]::Round($verifiedDisk.LargestFreeExtent / 1GB, 2)) GiB"
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
