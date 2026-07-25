# WSL DxCore/D3D12 backend

This backend is a WSL/DxCore GPU-PV proxy through Intel's signed Windows
driver. It runs real workloads on the physical A380, but it is not an
`atom_arc` native submission path and never claims raw PCI ownership.

## Verified sequence

1. Open `/dev/dxg` read/write.
2. Create an `IDXCoreAdapterFactory`.
3. enumerate D3D12 graphics adapters.
4. Read each adapter's `DXCoreHardwareID` and select exactly `8086:56a5`.
5. Create an `ID3D12Device` at feature level 11.0.
6. Create a compute command queue and allocator.
7. Create and serialize a three-parameter root signature containing an SRV,
   UAV, and two 32-bit constants.
8. Create a compute pipeline from DXC-built shader model 6.0 DXIL embedded at
   build time.
9. Create upload, default-heap input, default-heap UAV output, and readback
   resources with checked sizes.
10. Upload input values, copy them to the GPU input, and transition that input
    to `NON_PIXEL_SHADER_RESOURCE`.
11. Bind the SRV, UAV, and constants; dispatch enough 64-thread groups for the
    requested element count; transition the output; and copy it to readback.
12. Also record the independent 4,096-byte copy sentinel in the same command
    list, close it, and execute it on the selected Arc queue.
13. Signal a fence and poll it with a bounded timeout.
14. Require fence completion and a zero device-removal reason.
15. Verify every copy byte and every compute value plus both checksums.

The helper prints schema-1 JSON. The safe Rust bridge independently checks the
schema, backend name, PCI ID, every creation/submission/upload flag, input and
output byte counts, fence completion, 4,096 copied bytes, both checksums, and
device-removal result before it calls the submission verified.

The helper accepts 1 through 1,048,576 compute elements and a 32-bit addend.
The Rust runtime exposes this as registered kernel 1 with `[groups_x, 1, 1]`;
each group contains 64 threads. The stress command varies the addend on every
iteration so repeated runs cannot pass by returning a stale output buffer.

## Build

Ubuntu dependency:

```bash
sudo apt-get install directx-headers-dev g++ pkg-config
```

Build and execute:

```bash
native/dxcore/build-wsl.sh
target/wsl/release/atom-arc-dxcore-helper --vendor 0x8086 --device 0x56a5
target/wsl/release/atom-arcctl hardware-smoke
target/wsl/release/atom-arcctl compute --groups 16 --addend 7
target/wsl/release/atom-arcctl stress --groups 256 --iterations 10
```

`scripts/Test-AtomArcWsl.ps1` performs these steps automatically and makes
`doctor --require-hardware` a mandatory gate.
