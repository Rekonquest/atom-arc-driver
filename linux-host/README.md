# Native Ubuntu KVM/VFIO host

KVM/VFIO must run on native Linux. The intended topology keeps the RTX 5070 Ti
and any integrated display device available to the host and assigns the complete Intel Arc A380
GPU (`8086:56a5`) plus its display-audio function (`8086:4f92`) to the Atom
Ubuntu guest.

Download and checksum-verify the native host installer from Windows with:

```powershell
.\scripts\Get-AtomArcUbuntuHostIso.ps1
```

Run the scripts in numeric order on the native Ubuntu host:

```bash
sudo ./01-prepare-kvm-host.sh
sudo reboot
sudo ./02-bind-arc-vfio.sh
sudo reboot
./verify-vfio.sh
sudo ./03-create-atom-guest.sh /var/lib/libvirt/images/atom-arc-ubuntu.qcow2
```

Once the guest is running with the A380 attached, copy the current project into
the guest and run the separate `linux-guest` sequence:

```bash
sudo ./linux-guest/01-install-atom-driver.sh
sudo reboot
./linux-guest/02-verify-atom-driver.sh
sudo bash ./linux-guest/03-test-native-stage0.sh
```

This is the intended topology and command sequence, not a record of a
successful native run. Stage 0 remains pending until that exact guest command
writes fresh passing evidence.

Only the guest blacklists `i915` and `xe`. The native host continues using the
RTX 5070 Ti and UHD 770 normally while `vfio-pci` owns the A380 functions.
The guest retains a VirtIO/SPICE display for recovery; the passed-through A380
is not used for the guest console during Stage 0.

The binding script fails closed if the physical functions are missing, IOMMU
is disabled, or an unrelated endpoint shares either A380 IOMMU group. It does
not use an ACS override patch.

Before creating the guest, copy the already-provisioned QCOW2 disk from
`C:\Projects\atom-arc-driver\vm\runtime\atom-arc-ubuntu.qcow2` into
`/var/lib/libvirt/images` on the native Linux filesystem. Do not run a VM disk
directly from NTFS.

## Prepared E-drive installation layout

Windows Disk 2 (`Samsung SSD 990 EVO Plus 1TB`, serial fragment
`3851_5140_3BE1`) is the only native-Ubuntu target. The guarded Windows scripts
reserve exactly 200 GiB without changing the existing E: filesystem, stage the
installer, and create a separate EFI system partition for the installed host.

| Partition | Size | Purpose | Installer action |
| --- | ---: | --- | --- |
| 1 | 16 MiB | Microsoft reserved | Preserve; do not edit |
| 2 / E: | 731.5 GiB | Existing NTFS data | Preserve; never format |
| 3 / G: | 8 GiB | Bootable Ubuntu installer | Preserve during installation |
| 4 | 1 GiB | Installed Ubuntu EFI system partition | Select as `/boot/efi`; format FAT32 |
| Free space | 191 GiB | Native Ubuntu/KVM host | Create ext4 and mount at `/` |

In Subiquity, choose **Custom storage layout**. Do not use the whole-disk or
guided erase options. Confirm that partitions 1, 2, and 3 are not selected for
formatting. Assign partition 4 to `/boot/efi`, then create one ext4 root
partition from the entire 191 GiB free extent and mount it at `/`.

The Ubuntu installer lives on the same physical SSD as the target free space,
so automatic whole-disk storage selection is intentionally not used.
