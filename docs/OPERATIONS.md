# Operations

## Build and verify

Run the portable build and Windows/WSL proxy gate from PowerShell:

```powershell
cd C:\Projects\atom-arc-driver
.\scripts\Test-AtomArcBuild.ps1
.\scripts\Test-AtomArcKernelModule.ps1
```

The first script verifies the portable Rust control plane and legacy WSL
comparison backend. The second compiles `atom_arc.ko` against the exact Ubuntu
VM kernel, runs warnings-as-errors and Sparse, verifies the module contains no
firmware-loader dependency, firmware metadata, or module parameter, and then
loads and unloads it without a GPU.

Evaluate a captured paired experiment with:

```powershell
.\target\release\atom-arcctl.exe experiment-evaluate `
  --input .\experiments\my-candidate.json
```

Exit 0 means the record was valid and its verdict was `fixture-valid`,
`retain-for-native-validation`, or `promote`. Exit 1 rejects the candidate with
every failed gate, and exit 2 means the record itself was invalid. Synthetic
fixtures can never retain or promote a primitive. Live A380 comparison evidence
can retain it; only native `atom_arc` evidence can promote it. The schema and
phase rules are defined by `atom-arc-lab` and `docs/SILICON-LAB.md`.

Every record must include `baseline_stack` and `candidate_stack`. The candidate
must preserve the baseline order and append exactly the named primitive. A
reordered or bundled stack exits 2 before any performance result is promoted.

Run the exact physical-A380 WSL/DxCore GPU-PV proxy staging-pool matrix with:

```powershell
cd C:\Projects\atom-arc-driver
.\scripts\Test-AtomArcLivePrimitives.ps1
```

This rebuilds the WSL helper and Rust control plane, then writes five physical
A380 records under `experiments/results`. A failed correctness or performance
gate makes the script fail rather than silently preserving the candidate.

Run the GPU-PV proxy compound matrix against the retained staging pool with:

```powershell
cd C:\Projects\atom-arc-driver
.\scripts\Test-AtomArcCompoundPrimitives.ps1
```

This appends only `double-buffered-preload` to the frozen feasibility stack and
writes five `live-a380-compound-double-buffer-*.json` records. Exit 2 aborts on
an invalid or incomplete live capture. A statistical rejection is recorded,
reported after the complete matrix, and then fails the retained-stack gate.

## Install in the isolated A380 guest

After the native KVM/VFIO host passes the A380 into the Atom guest:

```bash
cd /path/to/atom-arc-driver
sudo ./linux-guest/01-install-atom-driver.sh
sudo reboot
./linux-guest/02-verify-atom-driver.sh
sudo bash ./linux-guest/03-test-native-stage0.sh
```

The installer aborts if any Intel display device other than the one A380 is
present. It blacklists `i915` and `xe` only inside that dedicated guest and does
not live-unbind the running GPU driver.

The Stage 0 gate rebuilds the exact module and requires two clean bind/unbind
cycles, direct A380 PCI identity, BAR0/BAR2 resources, mapped BAR0, the 46-bit
DMA capability, zero GPU MMIO reads, firmware not started, and PCI bus
mastering disabled. It does not test firmware, interrupts, resets, submission,
or performance.

## Legacy WSL comparison

`atom-arcctl-wsl.cmd` forwards commands to the release binary in Ubuntu. With
no arguments it runs the hardware-required doctor.

```powershell
.\atom-arcctl-wsl.cmd
.\atom-arcctl-wsl.cmd probe
.\atom-arcctl-wsl.cmd hardware-smoke
.\atom-arcctl-wsl.cmd compute --groups 16 --addend 7
.\atom-arcctl-wsl.cmd stress --groups 256 --iterations 10
.\atom-arcctl-wsl.cmd experiment-live-transfer --payload-bytes 1048576 `
  --operations 16 --samples 20 --warmups 5 `
  --output experiments/results/live-a380-reusable-staging-1m.json
.\atom-arcctl-wsl.cmd experiment-live-compound --payload-bytes 1048576 `
  --operations 16 --samples 20 --warmups 5 `
  --output experiments/results/live-a380-compound-double-buffer-1m.json
```

Add `--json` to `probe`, `hardware-smoke`, `compute`, `stress`, `self-test`,
`security-audit`, or `doctor` for automation-friendly output.

## Legacy dispatch contract

- Kernel ID: 1.
- Thread-group size: 64 threads in X.
- Grid: `[groups_x, 1, 1]`.
- Maximum output: 1,048,576 `u32` elements (16,384 groups).
- Bindings: one backend-owned SRV input and one backend-owned UAV output.
- Constant: one configurable wrapping `u32` addend.
- Completion: bounded fence wait, zero device-removal reason, full readback
  comparison, and an independently checked Rust checksum.

The current public dispatch is intentionally a registered validation kernel,
not an arbitrary-DXIL loader. This keeps the first usable build narrow while
the later shader and user-buffer ABI is designed.

## Exit behavior

- Exit 0: requested checks and independent verification passed.
- Exit 1: a complete report was produced but a readiness check failed.
- Exit 2: invalid arguments, missing helper, malformed report, or backend start
  failure.

Firmware mutation and authentication bypass remain disabled everywhere.
Userspace PCI configuration and raw MMIO remain disabled. Stage 0 keeps bus
mastering disabled and performs no GPU MMIO access; all later capabilities stay
outside the active stack until their preceding hardware gates pass.
