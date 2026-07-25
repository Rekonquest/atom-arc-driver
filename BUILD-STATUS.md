# Build status

Verified on 2026-07-15 at 17:23:15 America/New_York.

## Exact current scope

The active deliverable is a strict Stage 0 Intel Arc A380 (`8086:56a5`)
probe-only Linux PCI driver. Its source and build lanes pass, but it has not
owned the physical A380 in a native KVM/VFIO guest. It is not a functional
compute, render, or display driver yet.

`atom_arc.ko` is independent: it does not call, wrap, or link `i915`, `xe`,
WDDM, DxCore, or D3D12. The WSL/DxCore code is retained as a separately labeled
Windows-driver GPU-PV proxy and never counts as native-driver evidence.

## Active Stage 0 stack

The enforced order is:

1. match only Intel `8086:56a5`;
2. enable PCI memory resources;
3. clear and read back the PCI bus-master bit;
4. claim all PCI regions;
5. configure the required 46-bit coherent DMA mask without allocating or
   transferring DMA memory;
6. map BAR0 without dereferencing it;
7. expose read-only PCI identity, BAR, ownership, and lock state; and
8. unmap, release, and unload cleanly.

DG2 is a pre-GMDID platform in upstream Xe. Stage 0 therefore uses the safe PCI
vendor/device, subsystem, revision, class, and command-register fields for
identification rather than inventing an unsourced BAR0 read.

The active module links only `atom_arc_main.o`. It contains no firmware loader,
firmware metadata, module parameter, GPU MMIO accessor, IRQ allocation, DMA
allocation/map, reset, bus-master enable, DRM/UAPI, or submission path. Earlier
firmware-parser source is preserved but unlinked for the later ordered firmware
stage.

The source-policy gate fails the build if that stacking order changes or any
Stage 1+ API appears; unreadable sources or failed/missing inspection tools
also fail closed. The native Stage 0 evidence gate additionally requires
two exact-module bind/probe/unbind cycles, the verified board identity
`172f:3943` revision `0x05`, positive BAR0/BAR2 resources, 46-bit DMA
capability, zero GPU MMIO accesses, bus mastering off, and two clean unloads.

## Fresh verification

- `scripts/Test-AtomArcBuild.ps1 -SkipLivePrimitives`: exit 0 in 62.3 seconds.
- Windows Rust format, warnings-denied Clippy, release build, and 48 unit tests:
  pass.
- Ubuntu 24.04 development VM Kbuild with `W=1`, Sparse `C=2`, module metadata,
  source/symbol policy, no-GPU load, and unload: pass.
- WSL Ubuntu build and 47 tests: pass.
- Ubuntu 24.04 Docker build, warnings-denied Clippy, 47 tests, release build,
  security audit, reference smoke, and doctor: pass (`ATOM_ARC_DOCKER_READY`).
- Per-crate 4,000-line policy: pass; the largest crate is `atom-arcctl` at 999
  Rust source lines.
- All PowerShell files parse; all host/guest shell files pass `bash -n`; the
  Stage 0 Python evidence generator compiles and executes against a complete
  simulated argument set.
- The strict Stop hook passes 23 regression cases. It rejects mixed hook/native
  test wording, arbitrary integration-test paths, non-A380 identities, stale
  timestamps, mismatched hashes/run IDs, the obsolete zero-reset contract, and
  any disabled full-compute gate.
- Current Windows PnP inventory confirms the physical GPU
  `8086:56a5`, subsystem `172f:3943`, revision `05`, and its audio function
  `8086:4f92`, subsystem `172f:3943`, revision `00`.

The WSL GPU-PV proxy selected the physical A380 and passed copy, fixed compute,
fencing, and three-dispatch stress through Intel's signed Windows driver. This
is a proxy-path comparison and not an Atom native driver test.

## Primitive experiment status

The reusable staging pool remains retained only in the WSL GPU-PV feasibility
stack. Its current five-payload records report mean improvements from 12.255%
to 913.696%, with all paired correctness digests matching.

Appending `double-buffered-preload` above that frozen pool remains retained in
the same proxy stack. The fresh five-payload rerun reports 24.218% to 99.408%
mean improvement, 100 matching paired digests, and lower p95 latency at every
size.

The next appended candidate, `third-lane-preload`, failed four of five payload
classes and was removed from the executable path. Its five negative records
remain under `experiments/results/proxy-a380-third-lane-*.json`. None of these
proxy results is promoted into the native driver.

## Current artifacts

- Windows CLI: `target/release/atom-arcctl.exe`
  - 1,352,192 bytes
  - SHA-256 `201a4a295cd98c080fd45b4549aca3a38b5347ae7bef3d02516ebd9abaf881d6`
- WSL CLI: `target/wsl/release/atom-arcctl`
  - 1,276,640 bytes
  - SHA-256 `bcf5609d5167ae6ee8a635c5da7b5b7bb16697141bf2a4d9f8cef018468da8df`
- WSL proxy helper: `target/wsl/release/atom-arc-dxcore-helper`
  - 84,560 bytes
  - SHA-256 `648412a4016ca66dcc63f428b6d43a46bfad47337da29c434e0c1a68923b697c`
- Stage 0 module: `target/kernel/atom_arc-6.8.0-134-generic.ko`
  - 380,240 bytes
  - SHA-256 `c3078faee14fc6a8615931a430c9298bec6fb014f1602bc28889482542dd89f0`
- Docker image: `atom-arc:dev`
  - 30,315,738 bytes
  - digest `sha256:bdc2cc3279c4f92f098447cd795133e389848f5aab6fb9cb81bab784dd138bd1`

## Physical Stage 0 gate

The exact `linux-guest/03-test-native-stage0.sh` command was freshly invoked in
the current Ubuntu WHPX development VM. It exited 1 before probe with:

```text
STAGE0_BLOCKED: expected one passed-through A380 (8086:56a5); found 0.
```

That failure is not a passed hardware test. No
`atom-arc-native-stage0-latest.json`, `atom-arc-native-live-proof.json`, or
Stage 3 evidence file exists locally or in that VM.

Current Windows disk inspection confirms Disk 2 is the 1 TB Samsung target:
E: remains 731.497 GiB, the FAT32 G: installer is 8 GiB and contains
`EFI/BOOT/BOOTX64.EFI`, the separate Ubuntu ESP is 1 GiB, and exactly 191 GiB
remains unallocated for the native Ubuntu root filesystem.

The remaining physical requirement is to boot the prepared native Ubuntu host,
verify the A380 GPU and audio IOMMU groups under `vfio-pci`, attach both
functions to the KVM guest, and run the exact Stage 0 guest gate there. Docker,
WSL, and the Windows-hosted QEMU VM cannot substitute for that ownership path.
