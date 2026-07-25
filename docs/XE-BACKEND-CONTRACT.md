# Superseded native Xe backend contract

This document is retained as historical context. The `drm/xe` transport is no
longer the primary Atom architecture because it would place Atom above Intel's
existing Arc kernel driver. The independent `kernel/atom_arc` module replaces
this plan.

The legacy experimental backend would use the upstream DRM Xe UAPI. It must not directly
program firmware or undocumented MMIO registers.

## Enablement sequence

1. Locate an Intel render node and prove its sysfs driver is `xe`.
2. Open the render node without acquiring display ownership.
3. Query device configuration and memory regions through the Xe device-query
   ioctl.
4. Create a GEM buffer and VM using checked sizes from `atom-arc-security`.
5. Bind the buffer into the VM as a separate, fenced operation.
6. Create an execution queue for an advertised engine only.
7. Submit a known no-op batch and wait on an explicit synchronization object.
8. Destroy resources in reverse order and prove stale handles fail.
9. Enable fill/copy only after the no-op path passes reset and timeout tests.
10. Keep dispatch disabled until command encoding and engine-class validation
    are independently tested on the Arc A380.

## Required failure behavior

- Missing `xe` render node: unavailable, no fallback to raw PCI access.
- Unknown device or engine class: rejected.
- Unsupported UAPI version or extension: rejected.
- Timeout, reset, or wedged device: stop submission and surface diagnostics.
- Authentication or firmware fault: report only; never patch or bypass it.

Reference documentation:

- <https://docs.kernel.org/gpu/xe/>
- <https://docs.kernel.org/next/gpu/xe/xe_cs.html>
- <https://docs.kernel.org/gpu/xe/xe_firmware.html>
