[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path -Parent $PSScriptRoot
$image = 'atom-arc:dev'

& docker build --tag $image $root
if ($LASTEXITCODE -ne 0) { throw 'Atom Arc Docker image build failed' }

& docker run --rm $image probe
if ($LASTEXITCODE -ne 0) { throw 'Docker probe failed' }
& docker run --rm $image security-audit
if ($LASTEXITCODE -ne 0) { throw 'Docker security audit failed' }
& docker run --rm $image self-test
if ($LASTEXITCODE -ne 0) { throw 'Docker self-test failed' }
& docker run --rm $image doctor
if ($LASTEXITCODE -ne 0) { throw 'Docker doctor failed' }

Write-Host 'ATOM_ARC_DOCKER_READY'
