[CmdletBinding()]
param(
    [ValidateSet('whpx', 'tcg')]
    [string]$Accelerator = 'whpx',
    [ValidateRange(2, 32)]
    [int]$CpuCount = 8,
    [ValidateRange(4, 64)]
    [int]$MemoryGB = 16,
    [switch]$WaitForSsh
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$runtimeRoot = Join-Path $projectRoot 'vm\runtime'
$qemu = 'C:\Program Files\qemu\qemu-system-x86_64.exe'
$diskImage = Join-Path $runtimeRoot 'atom-arc-ubuntu.qcow2'
$seedImage = Join-Path $runtimeRoot 'seed.iso'
$pidPath = Join-Path $runtimeRoot 'qemu.pid'
$serialLog = Join-Path $runtimeRoot 'serial.log'
$qemuOutputLog = Join-Path $runtimeRoot 'qemu.stdout.log'
$qemuErrorLog = Join-Path $runtimeRoot 'qemu.stderr.log'

foreach ($required in $qemu, $diskImage, $seedImage) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Missing $required. Run .\scripts\New-AtomArcUbuntuVm.ps1 first."
    }
}

if (Test-Path -LiteralPath $pidPath) {
    $oldPid = [int](Get-Content -Raw -LiteralPath $pidPath)
    if (Get-Process -Id $oldPid -ErrorAction SilentlyContinue) {
        Write-Host "Atom Arc Ubuntu VM is already running as PID $oldPid"
        exit 0
    }
    Remove-Item -LiteralPath $pidPath -Force
}

$machine = if ($Accelerator -eq 'whpx') { 'q35,accel=whpx' } else { 'q35,accel=tcg' }
$arguments = @(
    '-name', 'atom-arc-ubuntu',
    '-machine', $machine,
    '-smp', "$CpuCount",
    '-m', "${MemoryGB}G",
    '-drive', "file=$diskImage,if=virtio,format=qcow2",
    '-drive', "file=$seedImage,media=cdrom,readonly=on,format=raw",
    '-device', 'virtio-net-pci,netdev=net0',
    '-netdev', 'user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22',
    '-device', 'virtio-rng-pci',
    '-display', 'none',
    '-serial', "file:$serialLog"
)

$process = Start-Process -FilePath $qemu -ArgumentList $arguments -PassThru `
    -WindowStyle Hidden -RedirectStandardOutput $qemuOutputLog -RedirectStandardError $qemuErrorLog
Start-Sleep -Seconds 2
if ($process.HasExited) {
    $details = if (Test-Path -LiteralPath $qemuErrorLog) {
        Get-Content -Raw -LiteralPath $qemuErrorLog
    } else {
        ''
    }
    throw "QEMU exited during startup. $details"
}
[IO.File]::WriteAllText($pidPath, "$($process.Id)`n", [Text.UTF8Encoding]::new($false))
Write-Host "Atom Arc Ubuntu VM started with $Accelerator as PID $($process.Id)"
Write-Host 'SSH: .\scripts\Connect-AtomArcUbuntuVm.ps1'

if ($WaitForSsh) {
    $deadline = [DateTime]::UtcNow.AddMinutes(12)
    do {
        Start-Sleep -Seconds 5
        $probe = Test-NetConnection -ComputerName 127.0.0.1 -Port 2222 -WarningAction SilentlyContinue
    } until ($probe.TcpTestSucceeded -or [DateTime]::UtcNow -ge $deadline -or $process.HasExited)
    if (-not $probe.TcpTestSucceeded) {
        throw "VM did not expose SSH before the timeout. Inspect $serialLog"
    }
    Write-Host 'SSH port is accepting connections.'
}
