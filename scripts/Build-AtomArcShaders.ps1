[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root 'native\dxcore\shaders\atom_arc_compute.hlsl'
$outputDirectory = Join-Path $root 'target\wsl\generated'
$dxil = Join-Path $outputDirectory 'atom_arc_compute.dxil'
$header = Join-Path $outputDirectory 'atom_arc_compute_dxil.h'

$dxc = Get-ChildItem -LiteralPath 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter 'dxc.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\dxc\.exe$' } |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if (-not $dxc) { throw 'The x64 DirectX Shader Compiler was not found in the Windows SDK' }

New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
& $dxc.FullName `
    -T cs_6_0 `
    -E main `
    -HV 2021 `
    -O3 `
    -Ges `
    -WX `
    -Fo $dxil `
    -Fh $header `
    -Vn g_atom_arc_compute_dxil `
    $source
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc compute shader compilation failed' }
if (-not (Test-Path -LiteralPath $dxil) -or (Get-Item -LiteralPath $dxil).Length -eq 0) {
    throw 'DXC did not produce a non-empty DXIL object'
}
if (-not (Test-Path -LiteralPath $header) -or (Get-Item -LiteralPath $header).Length -eq 0) {
    throw 'DXC did not produce a non-empty embedded shader header'
}

$hash = Get-FileHash -Algorithm SHA256 -LiteralPath $dxil
Write-Host "ATOM_ARC_SHADER_READY compiler=$($dxc.FullName) sha256=$($hash.Hash)"
