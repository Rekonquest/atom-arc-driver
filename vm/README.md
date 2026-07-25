# Atom Arc Ubuntu VM

This is a reproducible Ubuntu 24.04 LTS development VM for building and testing
the Atom Arc Linux code. It uses the QEMU installation already present on the
Windows workstation.

## Create and boot

```powershell
cd C:\Projects\atom-arc-driver
.\scripts\New-AtomArcUbuntuVm.ps1
.\scripts\Start-AtomArcUbuntuVm.ps1 -WaitForSsh
.\scripts\Sync-AtomArcUbuntuVm.ps1
.\scripts\Test-AtomArcUbuntuVm.ps1
.\scripts\Connect-AtomArcUbuntuVm.ps1
```

The VM has 8 vCPUs, 16 GB RAM, an 80 GB copy-on-write disk, and SSH forwarded
to `127.0.0.1:2222`. Cloud-init creates the `atomdev` account and installs the
kernel, DRM, Rust, C, and C++ build dependencies. Generated disks, credentials,
logs, and caches live under `vm/runtime` and are ignored by Git.

Re-run `Sync-AtomArcUbuntuVm.ps1` whenever the Windows source tree changes. It
overlays the current source into `/home/atomdev/atom-arc-driver` without
deleting guest-side work.

Stop it cleanly with:

```powershell
.\scripts\Stop-AtomArcUbuntuVm.ps1
```

## Hardware boundary

QEMU on this Windows 11 client host presents virtual hardware. It does not pass
the physical Arc A380 PCI function into the guest. This VM compiles
`atom_arc.ko`, runs Sparse, loads it without an A380, and exercises the real
production DG2 GuC/HuC container parsers through the module's firmware self-test.
Binding Atom to the actual A380 still requires the native Linux KVM/VFIO host.
`scripts/Test-AtomArcNativePhase0.ps1` intentionally fails in this QEMU/WHPX
development VM because the guest has no `8086:56a5` PCI function. A successful
module build or firmware-parser self-test here is not native-driver evidence.
