# Architecture

## Runtime boundary

Atom Arc separates its OS-agnostic A380 hardware policy from the current Linux
validation adapter, the portable Rust control plane, and the legacy WSL
comparison transport. Atom OS's agnostic kernel interfaces are the canonical
boundary. Ubuntu will map that boundary onto Linux when the native KVM/VFIO
guest owns the A380; that physical gate has not passed yet.

```text
atom-arcctl
    |
    +-- platform probe -------- native A380 PCI function + atom_arc sysfs
    |                         |- Windows/WSL legacy diagnostics
    |                         `- reference runtime
    |
    +-- security policy ------- production signed firmware required
    |                         |- userspace PCI/MMIO denied
    |                         `- submission locked before hardware auth
    |
    +-- runtime --------------- allocation and stale-handle safety
    |                         |- whole-buffer command validation
    |                         |- CPU reference execution
    |                         `- native submission unavailable until auth
    |
    +-- hardware core --------- ordered A380 state and security policy
    |                         |- modeled future firmware/authentication state
    |                         |- modeled future memory, reset, and submission rules
    |                         `- no OS-specific ownership policy
    |
    +-- Ubuntu adapter -------- atom_arc.ko, direct PCI ownership of 8086:56a5
    |                         |- active Stage 0: BAR0 map + DMA capability
    |                         |- zero MMIO, firmware, IRQ, reset, or submission
    |                         `- bus mastering forced and verified off
    |
    +-- Atom OS binding ------- canonical agnostic kernel primitives
    |
    `-- legacy DxCore --------- WSL-only comparison backend
```

## Core and adapter boundary

The A380 register model, firmware/authentication state machine, memory and
engine rules, reset behavior, command validation, primitive stack, and
telemetry schema belong to the common hardware core. They are not ported or
forked for Atom OS. `crates/atom-arc-core` is a `no_std` implementation of the
Phase 0 state machine and defines the kernel-adapter trait without importing
Linux or Windows APIs.

The current `atom_arc.ko` maps only Stage 0 PCI discovery/ownership, BAR0
mapping, DMA-address capability checking, and probe telemetry onto Linux APIs.
It is not an
upper layer over `i915` or `xe`; it registers its own PCI driver, has no module
dependencies, and claims only the A380 device ID. These direct Linux calls are
the validation adapter, not the canonical driver interface.

Atom OS already supplies OS-agnostic forms of those kernel facilities, so its
integration is a binding of the common core, not a conventional driver port.
Linux/Ubuntu is the compatibility mapping used to run the same logic in the VM.

The active module does not link the retained firmware-parser source and exposes
no firmware parameter or metadata. Firmware loading and physical GSC/GuC
authentication begin only after Stage 0 and the infrastructure stage have
passed on the VFIO-owned card.

## Why WSL and Docker remain separate legacy gates

WSL exposes the Windows host GPU through `/dev/dxg` and GPU-PV. The Windows
WDDM kernel driver still owns scheduling, memory management, and firmware.
Docker Desktop can build and run the portable layers, but the current Docker
engine does not expose `/dev/dxg`. Therefore:

- Docker proves a clean Ubuntu build and reference behavior.
- WSL proves Linux compatibility and executes a fenced D3D12 command list on
  the Arc A380 through `/dev/dxg` and the signed host driver.
- Native Linux KVM/VFIO with the A380 attached is required for Atom's own PCI,
  firmware-authentication, reset, and submission tests.

## Submission model

Every command buffer is validated in full before execution. A later invalid
command cannot partially execute earlier commands. Buffer identifiers are
monotonic and never reused within a process, so a freed handle cannot silently
refer to a new allocation.

The current commands are `noop`, `fill`, `copy`, `barrier`, and `dispatch`.
Fill and copy execute in the deterministic reference runtime. Dispatch is
explicitly routed to the WSL hardware backend with `submit_hardware`; it never
silently falls back to the CPU path. Registered kernel 1 uses 64-thread X
groups, a backend-owned input/output binding pair, a configurable addend, and a
maximum of 1,048,576 elements. Unknown kernels, Y/Z grids, multiple dispatches,
and user-buffer bindings fail closed.

The hardware path uploads deterministic input values, transitions the input to
an SRV, executes embedded DXIL into a UAV, transitions the output for copy,
waits on a bounded fence, checks the device-removal result, and compares every
readback value. The Rust bridge repeats the expected checksum calculation and
rejects incomplete or inconsistent native reports.

## Crate sizing

Each feature has its own crate. The build gate measures Rust source lines and
fails if any crate reaches 4,000 lines.
