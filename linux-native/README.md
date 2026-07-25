# Direct Pop!_OS hardware lane

This lane prepares the physical Pop!_OS workstation for the Arc A380 while
keeping the Alder Lake iGPU as the recovery display. It does not blacklist
`xe` or `i915`, add `atom_arc` to the initramfs, or install the experimental
module under `/lib/modules`.

After powering the machine off and replacing the NVIDIA card:

1. Connect the monitor to the motherboard display output for the first boot.
2. Boot Pop!_OS and run `atom-arc-after-swap`.
3. Confirm the A380 is bound to the stock `xe` driver and the baseline tools
   detect it.
4. Only after the baseline passes, run
   `sudo atom-arc-stage0-host-smoke` to perform two guarded Atom Stage 0
   ownership cycles.

The Stage 0 host smoke refuses to run if the A380 is the boot VGA device, if
the iGPU has no connected display, or if a process has the A380 DRM nodes
open. Its exit trap restores the original stock driver after the test.

This direct-host lane is hardware evidence, but it remains separately labeled
from the isolated-guest promotion gate.
