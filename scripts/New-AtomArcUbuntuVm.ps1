[CmdletBinding()]
param(
    [ValidateRange(20, 512)]
    [int]$DiskSizeGB = 80
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$vmRoot = Join-Path $projectRoot 'vm'
$runtimeRoot = Join-Path $vmRoot 'runtime'
$cacheRoot = Join-Path $runtimeRoot 'cache'
$toolsRoot = Join-Path $runtimeRoot 'tools'
$cloudInitRoot = Join-Path $vmRoot 'cloud-init'
$qemuImg = 'C:\Program Files\qemu\qemu-img.exe'
$baseName = 'noble-server-cloudimg-amd64.img'
$baseImage = Join-Path $cacheRoot $baseName
$diskImage = Join-Path $runtimeRoot 'atom-arc-ubuntu.qcow2'
$seedImage = Join-Path $runtimeRoot 'seed.iso'
$keyPath = Join-Path $runtimeRoot 'id_ed25519'
$checksumUrl = 'https://cloud-images.ubuntu.com/noble/current/SHA256SUMS'
$imageUrl = "https://cloud-images.ubuntu.com/noble/current/$baseName"

if (-not (Test-Path -LiteralPath $qemuImg)) {
    throw "QEMU is not installed at $qemuImg"
}

New-Item -ItemType Directory -Force -Path $runtimeRoot, $cacheRoot | Out-Null

$checksumResponse = Invoke-WebRequest -UseBasicParsing -Uri $checksumUrl
$checksumText = if ($checksumResponse.Content -is [byte[]]) {
    [Text.Encoding]::UTF8.GetString($checksumResponse.Content)
} else {
    [string]$checksumResponse.Content
}
$checksumMatch = [regex]::Match(
    $checksumText,
    "(?m)^([0-9a-f]{64})\s+\*$([regex]::Escape($baseName))\r?$"
)
if (-not $checksumMatch.Success) {
    throw "Could not locate $baseName in Ubuntu SHA256SUMS"
}
$expectedHash = $checksumMatch.Groups[1].Value.ToUpperInvariant()

$needsDownload = -not (Test-Path -LiteralPath $baseImage)
if (-not $needsDownload) {
    $currentHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $baseImage).Hash
    $needsDownload = $currentHash -ne $expectedHash
}
if ($needsDownload) {
    $partialImage = "$baseImage.partial"
    Remove-Item -LiteralPath $partialImage -Force -ErrorAction SilentlyContinue
    Write-Host "Downloading official Ubuntu 24.04 LTS cloud image..."
    & curl.exe -L --fail --retry 3 --output $partialImage $imageUrl
    if ($LASTEXITCODE -ne 0) {
        throw "Ubuntu image download failed with exit code $LASTEXITCODE"
    }
    $downloadedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $partialImage).Hash
    if ($downloadedHash -ne $expectedHash) {
        throw "Ubuntu image checksum mismatch: expected $expectedHash, got $downloadedHash"
    }
    Move-Item -LiteralPath $partialImage -Destination $baseImage -Force
}

$verifiedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $baseImage).Hash
if ($verifiedHash -ne $expectedHash) {
    throw "Cached Ubuntu image checksum mismatch"
}
Write-Host "Ubuntu image SHA-256 verified: $verifiedHash"

if (-not (Test-Path -LiteralPath $keyPath)) {
    # Windows PowerShell omits a truly empty native argument, so pass an
    # explicitly quoted empty passphrase to OpenSSH.
    & ssh-keygen.exe -q -t ed25519 -N '""' -C 'atom-arc-ubuntu-vm' -f $keyPath
    if ($LASTEXITCODE -ne 0) {
        throw "SSH key generation failed with exit code $LASTEXITCODE"
    }
}

$publicKey = (Get-Content -Raw -LiteralPath "$keyPath.pub").Trim()
$template = Get-Content -Raw -LiteralPath (Join-Path $cloudInitRoot 'user-data.template')
if (-not $template.Contains('__ATOM_VM_SSH_PUBLIC_KEY__')) {
    throw 'Cloud-init public-key marker is missing'
}
$renderedUserData = $template.Replace('__ATOM_VM_SSH_PUBLIC_KEY__', $publicKey)
$userDataPath = Join-Path $runtimeRoot 'user-data'
[IO.File]::WriteAllText($userDataPath, $renderedUserData, [Text.UTF8Encoding]::new($false))

$venvPython = Join-Path $toolsRoot 'Scripts\python.exe'
if (-not (Test-Path -LiteralPath $venvPython)) {
    py -3 -m venv $toolsRoot
}
& $venvPython -m pip install --disable-pip-version-check --quiet 'pycdlib==1.16.0'
if ($LASTEXITCODE -ne 0) {
    throw "pycdlib installation failed with exit code $LASTEXITCODE"
}
& $venvPython (Join-Path $vmRoot 'make_seed.py') `
    --user-data $userDataPath `
    --meta-data (Join-Path $cloudInitRoot 'meta-data') `
    --output $seedImage
if ($LASTEXITCODE -ne 0) {
    throw "Cloud-init seed creation failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path -LiteralPath $diskImage)) {
    & $qemuImg create -q -f qcow2 -F qcow2 -b $baseImage $diskImage "${DiskSizeGB}G"
    if ($LASTEXITCODE -ne 0) {
        throw "VM disk creation failed with exit code $LASTEXITCODE"
    }
}

Write-Host 'Atom Arc Ubuntu VM assets are ready.'
Write-Host "Disk: $diskImage"
Write-Host "SSH key: $keyPath"
Write-Host 'Start it with .\scripts\Start-AtomArcUbuntuVm.ps1'
