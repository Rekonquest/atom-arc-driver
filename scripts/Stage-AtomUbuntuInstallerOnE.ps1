[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    throw 'Installer staging must run from an elevated PowerShell process.'
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$runtimeRoot = Join-Path $projectRoot 'vm\runtime'
$isoPath = Join-Path $runtimeRoot 'installer\ubuntu-24.04.4-live-server-amd64.iso'
$logPath = Join-Path $runtimeRoot 'stage-ubuntu-installer-on-E.log'
$copyLogPath = Join-Path $runtimeRoot 'stage-ubuntu-installer-copy.log'
$expectedSerialFragment = '3851_5140_3BE1'
$expectedDataGuid = [guid]'0e457a10-0ee3-4582-a75d-f8664e1f6758'
$efiType = '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}'
$installerSize = 8GB
$succeeded = $false
$mountedIso = $null

if (-not (Test-Path -LiteralPath $isoPath)) {
    throw "Verified Ubuntu installer is missing: $isoPath"
}

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

    $dataPartition = Get-Partition -DiskNumber $disk.Number | Where-Object {
        [guid]$_.Guid -eq $expectedDataGuid
    }
    if (@($dataPartition).Count -ne 1) {
        throw 'Safety check failed: the preserved E: data partition was not found.'
    }
    if ($disk.LargestFreeExtent -lt 199GB) {
        throw "Expected the reserved 200 GiB extent; found only $([math]::Round($disk.LargestFreeExtent / 1GB, 2)) GiB."
    }

    $existingInstaller = Get-Volume | Where-Object {
        $_.FileSystemLabel -eq 'ATOM_UBUNTU_INSTALL'
    }
    if ($existingInstaller) {
        throw 'An ATOM_UBUNTU_INSTALL volume already exists; refusing to create a duplicate.'
    }

    $mountedIso = Mount-DiskImage -ImagePath $isoPath -PassThru
    $isoVolume = $mountedIso | Get-Volume
    if (-not $isoVolume.DriveLetter) {
        throw 'Mounted Ubuntu ISO did not receive a drive letter.'
    }
    $isoRoot = "$($isoVolume.DriveLetter):\"

    $installerPartition = New-Partition `
        -DiskNumber $disk.Number `
        -Size $installerSize `
        -AssignDriveLetter
    $dataEnd = $dataPartition.Offset + $dataPartition.Size
    if ($installerPartition.Offset -lt $dataEnd) {
        throw 'Safety check failed: new installer partition overlaps the preserved E: partition.'
    }

    $installerVolume = Format-Volume `
        -Partition $installerPartition `
        -FileSystem FAT32 `
        -NewFileSystemLabel 'ATOM_UBUNTU_INSTALL' `
        -AllocationUnitSize 4096 `
        -Force `
        -Confirm:$false
    if (-not $installerVolume.DriveLetter) {
        throw 'Formatted installer partition did not retain a drive letter.'
    }
    $installerRoot = "$($installerVolume.DriveLetter):\"

    & robocopy.exe $isoRoot $installerRoot /E /COPY:DAT /DCOPY:T /R:2 /W:1 /NFL /NDL /NP "/LOG:$copyLogPath"
    if ($LASTEXITCODE -ge 8) {
        throw "Ubuntu installer copy failed with robocopy exit code $LASTEXITCODE"
    }

    $sourceFiles = Get-ChildItem -File -Recurse -LiteralPath $isoRoot
    $targetFiles = Get-ChildItem -File -Recurse -LiteralPath $installerRoot
    $sourceBytes = ($sourceFiles | Measure-Object -Property Length -Sum).Sum
    $targetBytes = ($targetFiles | Measure-Object -Property Length -Sum).Sum
    if ($sourceFiles.Count -ne $targetFiles.Count -or $sourceBytes -ne $targetBytes) {
        throw 'Installer file-count or byte-count verification failed.'
    }

    foreach ($relativePath in 'EFI\BOOT\BOOTX64.EFI', 'casper\ubuntu-server-minimal.ubuntu-server.installer.squashfs') {
        $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $isoRoot $relativePath)).Hash
        $targetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $installerRoot $relativePath)).Hash
        if ($sourceHash -ne $targetHash) {
            throw "Installer hash verification failed for $relativePath"
        }
    }

    Set-Partition `
        -DiskNumber $disk.Number `
        -PartitionNumber $installerPartition.PartitionNumber `
        -GptType $efiType
    Update-HostStorageCache

    $verifiedDisk = Get-Disk -Number $disk.Number
    if ($verifiedDisk.LargestFreeExtent -lt 191GB) {
        throw 'Expected at least 191 GiB to remain for the Ubuntu root filesystem.'
    }
    Write-Host "Verified installer files: $($sourceFiles.Count)"
    Write-Host "Verified installer bytes: $sourceBytes"
    Write-Host "Remaining unallocated extent: $([math]::Round($verifiedDisk.LargestFreeExtent / 1GB, 2)) GiB"
    $succeeded = $true
} catch {
    Write-Error $_
    throw
} finally {
    if ($mountedIso) {
        Dismount-DiskImage -ImagePath $isoPath -ErrorAction SilentlyContinue | Out-Null
    }
    Stop-Transcript | Out-Null
}

if ($succeeded) {
    exit 0
}
