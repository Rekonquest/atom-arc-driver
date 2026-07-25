# Native DxCore helper

`atom_arc_dxcore.cpp` is the narrow native ABI boundary used only in WSL. It
links the WSL-provided `libdxcore.so` and `libd3d12.so` against Ubuntu's
`directx-headers-dev` package.

The helper accepts only target vendor/device IDs and a bounded timeout. Its
single fixed workload copies and verifies a 4,096-byte pattern. It does not
expose firmware operations, PCI configuration writes, MMIO, arbitrary command
bytes, or shader loading.

Build from WSL:

```bash
./native/dxcore/build-wsl.sh
```
