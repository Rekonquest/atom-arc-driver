# Security contract

Atom Arc is designed to coexist with Intel's firmware security chain.

## Enforced invariants

- Authenticated firmware remains required.
- Firmware inspection may be read-only; blob writes and device flashing are
  denied.
- Firmware-authentication bypass is always denied.
- Userspace PCI configuration writes and raw MMIO are denied.
- The Stage 0 kernel module claims PCI regions, forces bus mastering off, and
  performs zero GPU MMIO reads or writes.
- Unsafe Rust is forbidden at the workspace lint level.
- Native DxCore calls are isolated in a small C++ helper across a JSON process
  boundary; Rust never dereferences a COM pointer.
- The WSL/D3D12 proxy dispatch accepts only registered kernel 1, a bounded X grid, and
  backend-owned bindings. Unknown kernels, arbitrary bindings, oversized
  dispatches, and unsupported transports return errors instead of falling back
  to an unvalidated path.
- Allocation and transfer sizes are bounded and overflow-checked.

## Firmware ownership

In Stage 0, `atom_arc.ko` owns the physical A380 PCI function but contains no
firmware-loader path. The retained parser source is not linked. In the later
ordered firmware stage, Atom may request fixed production filenames and parse
published Intel container formats, while the hardware GuC/GSC chain remains
the cryptographic authority. Container parsing can never establish hardware
authentication. Atom never implements a flasher or accepts a bypass.

On Windows/WSL, the legacy comparison path still leaves firmware ownership with
the signed Intel WDDM host driver.

## Adding privileged code

The DxCore helper uses Ubuntu's packaged Microsoft DirectX headers and the WSL
`libdxcore.so` and `libd3d12.so` libraries. It selects only the configured PCI
ID, submits a fixed copy sentinel and registered compute kernel, waits on a
bounded fence, verifies every readback value, and checks the device-removal
reason. It exposes no firmware, MMIO, PCI-write, arbitrary-command, or dynamic
shader-loading command.

FFI remains isolated in the helper process. Rust treats its JSON as untrusted
input and independently validates identity, sizes, state flags, checksums,
fence completion, and device health. Any future ioctl layer must follow the
same boundary, use published OS UAPIs, and retain the current default denials.
