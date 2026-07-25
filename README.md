# Atom Arc Driver

Atom Arc is an OS-agnostic driver architecture for the Intel Arc A380
(`8086:56a5`). Atom OS provides the canonical kernel abstractions for PCI,
MMIO, DMA/IOMMU, interrupts, firmware acquisition, time, and telemetry. The
current `atom_arc.ko` is the Ubuntu/KVM validation adapter: it maps those
responsibilities onto Linux services so the physical GPU can be tested in an
isolated VFIO guest. It does not call, wrap, or link against Intel's `i915`/`xe`
drivers or the Windows WDDM stack.

Intel's production DG2 GuC, HuC, and GSC security chain remains mandatory for
later stages. The active Stage 0 module contains no firmware loader or firmware
module parameter. Earlier parser work is preserved as unlinked source for the
ordered firmware stage; it cannot execute in Stage 0. Atom Arc never flashes
board firmware or bypasses firmware security.

## What works now

- A `no_std` future-stage policy model with security-register decoding,
  authentication regression detection, monotonic-heartbeat qualification, and
  reset relocking. Its self-test is in-memory only and is not hardware evidence.
- Independent `atom_arc.ko` PCI match for only `8086:56a5`.
- Direct BAR0 reservation/mapping and 46-bit coherent-DMA capability
  validation, with the inherited PCI bus-master bit cleared and verified.
- A probe-only Stage 0 path with zero GPU MMIO accesses and no firmware, IRQ,
  DMA-transfer, reset, user-API, or submission activity.
- Read-only Stage 0 PCI identity, BAR, mapping, and ownership state under the
  A380 PCI device's `atom_arc/` sysfs group.
- A no-GPU Kbuild/Sparse/load/unload gate that rejects firmware-loader symbols,
  firmware metadata, and module parameters, plus a KVM guest gate that refuses
  the wrong GPU topology.
- Arc A380-aware PCI and architecture model in the Rust control plane.
- Bounded command buffers with fill, copy, barrier, and no-op operations.
- Stale-handle-safe memory manager with allocation budgets and transactional
  pre-validation.
- Deterministic CPU reference backend for exercising driver semantics.
- Read-only probes for Windows WDDM, WSL `/dev/dxg`, Docker, and native Linux
  DRM render nodes bound to `xe`.
- A legacy WSL DxCore/D3D12 diagnostic backend that selects the Arc A380 by PCI ID; creates a
  root signature, compute pipeline, queue, resources, and command list; uploads
  an input buffer; dispatches embedded DXIL; and verifies the output readback.
- WSL/D3D12 proxy kernel-1 dispatch with bounded group counts, independent
  result validation, configurable constants, and a repeated stress command.
- Security audit that proves firmware mutation, firmware-authentication bypass,
  userspace PCI configuration writes, raw MMIO, and unsafe Rust are disabled.
- Reproducible Ubuntu 24.04 Docker build plus Windows and WSL verification.

The immediate hardware-validation target is Stage 0 on the whole A380 passed
through by KVM/VFIO to the Atom Ubuntu guest. Success is two clean
probe/bind/unbind cycles with real PCI/BAR evidence, zero MMIO accesses, and
bus mastering disabled. WSL GPU-PV remains only a proxy comparison path. IRQ,
DMA traffic, firmware boot, reset, and command submission are later ordered
stages and cannot be promoted by a Stage 0 result.

## Quick start

```powershell
cd C:\Projects\atom-arc-driver
cargo run -p atom-arcctl -- probe
cargo run -p atom-arcctl -- self-test
cargo run -p atom-arcctl -- security-audit
cargo run -p atom-arcctl -- doctor
```

Run the portable build and Windows/WSL proxy gate:

```powershell
.\scripts\Test-AtomArcBuild.ps1
```

Build and exercise the independent kernel module in the Ubuntu development VM:

```powershell
.\scripts\Test-AtomArcKernelModule.ps1
```

The following commands exercise the legacy WSL comparison transport; they are
not the standalone driver:

```powershell
.\atom-arcctl-wsl.cmd doctor --require-hardware
.\atom-arcctl-wsl.cmd compute --groups 16 --addend 7
.\atom-arcctl-wsl.cmd stress --groups 256 --iterations 10
```

Build and test the Ubuntu image:

```powershell
.\scripts\Test-AtomArcDocker.ps1
```

Run the Linux build and `/dev/dxg` probe in WSL:

```powershell
.\scripts\Test-AtomArcWsl.ps1
```

The WSL gate installs Ubuntu's packaged DirectX headers when needed, builds the
native helper, and runs:

```bash
./target/wsl/release/atom-arcctl hardware-smoke
./target/wsl/release/atom-arcctl compute --groups 16 --addend 7
./target/wsl/release/atom-arcctl stress --groups 256 --iterations 10
./target/wsl/release/atom-arcctl doctor --require-hardware
```

## Workspace layout

- `atom-arc-model`: hardware identifiers and bounded command model.
- `kernel/atom_arc`: independent Stage 0 A380 Linux PCI module; unlinked
  later-stage firmware-parser source is retained in the same directory.
- `linux-guest`: guarded module installation and live-device verification.
- `atom-arc-dxcore`: legacy Rust process bridge to the WSL native helper.
- `atom-arc-lab`: provenance-aware live experiment and stacking-order gate.
- `atom-arc-security`: immutable fail-closed policy.
- `atom-arc-platform`: read-only host and transport discovery.
- `atom-arc-runtime`: memory manager and reference executor.
- `atom-arcctl`: diagnostics, self-test, audit, and readiness gates.

See [architecture](docs/ARCHITECTURE.md), [security](docs/SECURITY.md), and the
[silicon-lab phase doctrine](docs/SILICON-LAB.md). The
[legacy WSL DxCore backend](docs/DXCORE-BACKEND.md) remains for comparison. The
exact operator commands and limits are in [operations](docs/OPERATIONS.md).

Synthetic experiment fixtures never count as optimization evidence. Run
`scripts\Test-AtomArcLivePrimitives.ps1` for the current physical-A380
Windows-driver GPU-PV fresh-versus-reusable staging-buffer matrix, then run
`scripts\Test-AtomArcCompoundPrimitives.ps1` for the ordered reusable-pool plus
double-buffered-preload comparison.
