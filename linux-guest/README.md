# Isolated Atom Ubuntu guest

These scripts run **inside** the KVM guest after the Arc A380 GPU and its audio
function have been passed through by the native Ubuntu host.

```bash
sudo ./01-install-atom-driver.sh
sudo reboot
./02-verify-atom-driver.sh
sudo bash ./03-test-native-stage0.sh
```

The installer refuses to proceed unless the guest contains exactly one Intel
display-class device and it is `8086:56a5`. This prevents the Atom module from
displacing an Intel display driver on the host or in a guest that was given the
wrong GPU.

The active gate is `03-test-native-stage0.sh`. It rebuilds and hash-checks the
exact module, performs two bind/probe/unbind cycles, and requires real A380 PCI
identity, BAR0/BAR2 resources, a 46-bit DMA capability, mapped BAR0, zero MMIO
accesses, bus mastering disabled, firmware not started, and clean unload. It
writes only `atom-arc-native-stage0-latest.json`; Stage 0 evidence cannot be
used as a firmware, interrupt, reset, submission, or performance claim.
It has not passed on the physical A380 yet, and no native Stage 0 evidence file
currently exists.

`04-test-native-phase0.sh` is a preserved legacy scaffold and is explicitly
disabled. The canonical full native compute gate will be Stage 3, after the
ordered infrastructure and firmware/memory stages genuinely pass.
