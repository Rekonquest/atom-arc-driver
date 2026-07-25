[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$installerRoot = Join-Path $projectRoot 'vm\runtime\installer'
$isoName = 'ubuntu-24.04.4-live-server-amd64.iso'
$isoPath = Join-Path $installerRoot $isoName
$releaseRoot = 'https://releases.ubuntu.com/24.04.4'
$checksumUrl = "$releaseRoot/SHA256SUMS"
$isoUrl = "$releaseRoot/$isoName"

New-Item -ItemType Directory -Force -Path $installerRoot | Out-Null
$response = Invoke-WebRequest -UseBasicParsing -Uri $checksumUrl
$checksumText = if ($response.Content -is [byte[]]) {
    [Text.Encoding]::UTF8.GetString($response.Content)
} else {
    [string]$response.Content
}
$match = [regex]::Match(
    $checksumText,
    "(?m)^([0-9a-f]{64})\s+\*$([regex]::Escape($isoName))\r?$"
)
if (-not $match.Success) {
    throw "Could not locate $isoName in Canonical's SHA256SUMS"
}
$expectedHash = $match.Groups[1].Value.ToUpperInvariant()

$download = -not (Test-Path -LiteralPath $isoPath)
if (-not $download) {
    $download = (Get-FileHash -Algorithm SHA256 -LiteralPath $isoPath).Hash -ne $expectedHash
}
if ($download) {
    $partial = "$isoPath.partial"
    Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
    Write-Host "Downloading $isoName..."
    & curl.exe -L --fail --retry 3 --output $partial $isoUrl
    if ($LASTEXITCODE -ne 0) {
        throw "Ubuntu installer download failed with exit code $LASTEXITCODE"
    }
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $partial).Hash
    if ($actualHash -ne $expectedHash) {
        throw "Ubuntu installer checksum mismatch: expected $expectedHash, got $actualHash"
    }
    Move-Item -LiteralPath $partial -Destination $isoPath -Force
}

$verifiedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $isoPath).Hash
if ($verifiedHash -ne $expectedHash) {
    throw 'Cached Ubuntu installer checksum mismatch'
}
Write-Host "Ubuntu installer verified: $isoPath"
Write-Host "SHA-256: $verifiedHash"
