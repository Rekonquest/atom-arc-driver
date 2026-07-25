# Atom Arc kernel driver

`atom_arc.ko` is an independent Linux PCI driver for the Intel Arc A380
(`8086:56a5`). It does not link to or invoke `i915`, `xe`, WDDM, DxCore, or a
DRM Xe render node.

The active Stage 0 hardware path deliberately stops after:

1. claiming only the A380 PCI function;
2. enabling its PCI memory resources;
3. clearing and reading back the PCI bus-master bit;
4. claiming all PCI regions;
5. establishing the required 46-bit DMA addressing capability without making
   a DMA allocation or transfer; and
6. mapping BAR0 without dereferencing it; and
7. identifying DG2 from the safe PCI vendor/device, subsystem, revision,
   class, and command-register fields; and
8. exposing PCI identity, BAR, mapping, and ownership state under the device's
   `atom_arc/` sysfs group.

Stage 0 performs zero GPU MMIO reads and writes. It does not request firmware,
allocate an interrupt, allocate DMA memory, reset the device, register a user
API, or submit commands. The RTX 5070 Ti remains the Ubuntu host display GPU;
the A380 must be passed through as a complete VFIO device to the development
guest before this stage can be called hardware-tested.

Upstream Xe classifies DG2 as a pre-GMDID platform, so it derives the graphics
IP descriptor from the PCI device description rather than probing the newer
GMD_ID register. Atom Arc follows that boundary instead of inventing an
unsourced BAR0 read merely to make the counter nonzero.

Structural reference: upstream Linux
[`xe_pci.c`](https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/xe/xe_pci.c).

The module clears PCI bus mastering at the start of probe, verifies the MASTER
bit stayed clear, and leaves it clear through removal and shutdown.

The Stage 0 module does not link the retained firmware-parser source. Its
module metadata has no firmware entries or parameters, and it has no
`request_firmware` dependency. That source is reserved for the later ordered
firmware stage and cannot execute now.

Build and run the no-GPU Stage 0 gate on Ubuntu with:

```bash
make
make sparse
test -z "$(modinfo -F firmware atom_arc.ko)"
test -z "$(modinfo -F parm atom_arc.ko)"
sudo insmod atom_arc.ko
sudo rmmod atom_arc
```
