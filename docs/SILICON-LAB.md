# Atom Arc silicon lab doctrine

Atom Arc is an experimental driver stack whose purpose is to expose more of the
Arc A380's usable silicon performance without weakening Intel's authenticated
firmware chain. Compatibility is a constraint, not the product identity.

## Operating rule

Only one driver phase is active at a time. Every phase follows the same loop:

1. establish a correctness-checked baseline on the physical A380;
2. import one cross-domain primitive;
3. change one controlled variable;
4. collect paired measurements over the same payload and operating state;
5. reject any output mismatch, timeout, unexpected reset, or device error;
6. promote only a statistically positive and practically meaningful result;
7. freeze the winning path as the next phase's baseline.

The default promotion gate is at least 10 paired samples, at least 10% mean and
median throughput improvement, a positive paired 95% confidence lower bound,
and no more than 5% p95 latency regression. A phase may set a stricter gate but
may not quietly weaken it after seeing results.

## Stacking-order doctrine

Primitive composition is non-commutative: `A -> B` and `B -> A` are different
driver designs and must never share a result. The exact order is part of the
experiment identity and is recorded in every report.

- Every baseline has a frozen, ordered primitive stack.
- A candidate may append exactly one primitive to that stack. It may not
  reorder, replace, or silently bundle primitives.
- After promotion, that candidate order becomes the next frozen baseline.
- Any removal, replacement, or reordering invalidates all downstream results
  and requires a new baseline plus new paired measurements.
- Interaction studies compare explicit orders independently and use ablation
  runs to identify whether the gain belongs to one primitive or their order.
- Security bring-up remains the bottom layer. No speed primitive may be placed
  beneath or bypass firmware authentication, reset recovery, or correctness.

The experiment evaluator enforces the append-only rule. A record whose
`candidate_stack` is not the exact `baseline_stack` plus the named primitive is
invalid rather than merely slow.

## Kernel integration target

Atom OS is the canonical native kernel home for the driver and already exposes
PCI lifecycle, MMIO, DMA/IOMMU, interrupts, firmware acquisition, time, and
telemetry through OS-agnostic kernel primitives. Atom Arc therefore does not
need a second hardware-driver port. The common A380 state machine and its exact
primitive stacking order bind directly to those interfaces.

The Ubuntu module is the pending compatibility adapter and isolated A380
measurement harness. It maps the same responsibilities onto Linux APIs for
KVM/VFIO tests once the native host is booted;
Linux-specific calls must not leak upward into the common hardware policy. A
similarly named local checkout must not be guessed as Atom OS when the actual
binding work begins.

## Layered data fabric

The Spiderweb model is used literally:

- Ground traffic is the flexible control path: allocation, faults, recovery,
  small transfers, and irregular commands.
- On-ramps classify repeatable traffic that can be promoted.
- Highway lanes are persistent directional rings for bulk data or commands.
- Off-ramps return completions, faults, and results to the control path.
- Preloading prepares the next transfer or command while the current one runs.
- Vibrations propagate queue pressure, engine stalls, thermal throttling,
  reset state, and memory pressure through every layer.
- Threads form from measured traffic. We do not permanently optimize a route
  until repeated flow proves that route exists.

## Hardware stacking gate

Hardware stages are distinct from the performance-research phases below and
must be completed in this order:

1. **Stage 0 — probe only (implemented, native pass pending):** direct A380 PCI ownership, resource
   claim, 46-bit DMA capability check, BAR0 mapping, PCI identity telemetry,
   and clean unload. Bus mastering is forced off. There are zero GPU MMIO
   accesses, firmware requests, IRQs, DMA transfers, resets, or submissions.
2. **Stage 1 — ownership infrastructure:** bounded IRQ allocation, DMA
   allocation/mapping, error paths, reset detection, timeouts, and structured
   state. No firmware or user submission is promoted here.
3. **Stage 2 — firmware and memory:** GuC, HuC, and GSC boot/authentication,
   VRAM/GGTT/GPUVM, 64-KiB constraints, fences, migration, and eviction.
4. **Stage 3 — headless compute/render:** fixed copy/compute command paths and
   completion on a DRM render node, with display still owned by the RTX.
5. **Stage 4 — userspace interface:** a deliberately small compute/render UAPI
   plus its open-source userspace implementation.
6. **Stage 5 — display last:** KMS, connectors, planes, link training, hotplug,
   atomic modesetting, and scanout.

No later-stage interface or capability may be left executable in an earlier
stage. WSL/DxCore measurements are proxy feasibility screens and cannot close
any hardware stage.

## Ordered phases

### Phase 0: secure bring-up and measurement

Future measurement prerequisite after hardware Stage 2. Direct A380 PCI ownership, production firmware loading,
hardware GuC/GSC authentication, reset recovery, monotonic timestamps, engine
and memory telemetry, and a zero-work heartbeat. No performance primitive may
advance before these measurements are trustworthy.

Exit gate: 1,000 consecutive authenticated heartbeat cycles, exactly one
induced reset with verified recovery, and zero unexpected resets, timeouts,
data mismatches, or firmware-state ambiguity.

### Phase 1: host-to-VRAM on-ramp

Baseline: one correctness-checked host-to-VRAM transfer at a time.

Candidate primitives, tested separately:

- Aedes reusable pinned-buffer pool instead of per-transfer pinning;
- CPU-cache-aligned and huge-page-backed staging;
- networking-style multiple persistent lanes and queue-depth control;
- storage-style scatter/gather batching;
- double/triple buffering for preload plus DMA overlap;
- cut-through chunk forwarding for streaming payloads.

Payload matrix: 4 KiB, 64 KiB, 1 MiB, 16 MiB, and 256 MiB with queue depths
1, 2, 4, 8, 16, and 32. Report throughput, p50/p95/p99 latency, CPU cost,
stalls, retries, and correctness digest.

#### Live feasibility screen: reusable staging-buffer pool

The first Aedes-derived candidate has been manually implemented and run on the
physical Arc A380 through the working WSL/DXCore GPU-PV comparison path. The
baseline creates and maps fresh upload/device/readback resources for every
transfer. The candidate reuses one persistently mapped staging pool. Queue,
allocator, command list, fence, payload generation, device-local round trip,
readback, and byte-for-byte FNV-64 correctness checking are otherwise the same.
Pair order alternates to reduce ordering bias.

These are end-to-end verified staging-path rates, using logical upload bytes as
the numerator. They include CPU payload generation, GPU upload and readback,
fence wait, and correctness verification; they are not raw PCIe bandwidth.

| Payload | Operations/sample | Pairs | Baseline B/s | Pool B/s | Mean | Median | Lower 95% | Baseline p95 ns | Pool p95 ns | Verdict |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 4 KiB | 64 | 20 | 3,021,167.642 | 30,547,965.638 | +913.696% | +916.293% | +884.175% | 97,725,486 | 9,698,691 | retain |
| 64 KiB | 32 | 20 | 43,703,668.980 | 260,033,355.373 | +499.361% | +493.172% | +474.294% | 56,520,107 | 8,480,494 | retain |
| 1 MiB | 16 | 20 | 271,163,934.012 | 472,476,598.062 | +74.527% | +73.344% | +70.906% | 72,362,774 | 36,084,201 | retain |
| 16 MiB | 4 | 20 | 463,367,745.991 | 520,077,636.609 | +12.255% | +11.752% | +11.571% | 152,283,077 | 132,509,972 | retain |
| 256 MiB | 1 | 20 | 430,100,763.610 | 553,434,791.458 | +28.671% | +29.053% | +27.886% | 658,939,716 | 516,097,501 | retain |

All 100 measured pairs completed without mismatch, timeout, reset, or device
removal. The candidate is retained in the removable experiment path because it
cleared the gate at every tested payload. It is not in the frozen native driver
stack and is not promoted until `atom_arc` owns the A380 and reproduces the
result above the Phase 0 security layer.

#### Live compound screen: reusable pool plus double-buffered preload

The next experiment froze the retained feasibility order as
`secure-bring-up -> reusable-staging-buffer-pool`, then appended exactly one
Spiderweb primitive: `double-buffered-preload`. The baseline reuses one
persistently mapped upload/device/readback lane and waits for each operation.
The candidate owns two persistent lanes and two fence/command contexts, so it
can prepare the next payload while the current GPU copy and readback remain in
flight. Resource and command-context setup is excluded from both timed paths.
The queue, payload, device-local round trip, alternating pair order, full
readback, and FNV-64 digest remain identical.

| Payload | Operations/sample | Pairs | Retained pool B/s | Double buffer B/s | Mean | Median | Lower 95% | Pool p95 ns | Double p95 ns | Verdict |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 4 KiB | 64 | 20 | 30,843,293.699 | 61,411,526.822 | +99.408% | +97.865% | +88.773% | 9,347,806 | 5,439,645 | retain |
| 64 KiB | 32 | 20 | 262,412,149.405 | 502,270,028.099 | +91.512% | +92.551% | +86.860% | 8,458,681 | 4,770,173 | retain |
| 1 MiB | 16 | 20 | 458,885,074.059 | 608,164,344.554 | +32.620% | +33.519% | +29.134% | 38,844,776 | 34,115,742 | retain |
| 16 MiB | 4 | 20 | 519,848,716.115 | 645,663,600.471 | +24.218% | +24.078% | +23.295% | 133,489,141 | 105,749,236 | retain |
| 256 MiB | 2 | 20 | 554,168,329.383 | 699,608,525.209 | +26.252% | +26.135% | +25.523% | 1,037,153,486 | 828,216,334 | retain |

All 100 compound pairs completed with exact output, fence completion, and a
healthy device. The compound cleared every throughput, confidence, and p95
gate. It is therefore retained above the reusable pool in the WSL feasibility
stack. This is still not native-driver promotion: Phase 0 must pass with the
A380 owned by `atom_arc`, after which the same ordered compound must be measured
again rather than inferred from GPU-PV.

The table above is the post-rejection rerun. Its five run IDs begin with
`wsl-dxcore-a380-compound-` and end in `1784147578`, `1784147580`,
`1784147586`, or `1784147625`; the exact IDs and paired measurements are in the
five `live-a380-compound-double-buffer-*.json` records.

#### Rejected compound: appended third preload lane

The next removable experiment preserved the exact retained order
`secure-bring-up -> reusable-staging-buffer-pool -> double-buffered-preload`
and appended one storage-queue-depth primitive, `third-lane-preload`. The
baseline owned two persistent transfer lanes; the candidate owned three. No
other stack element, queue, payload, pair ordering, readback, or correctness
check changed.

This was exercised on the physical Arc A380 through the Intel-owned WSL
DxCore/D3D12 GPU-PV path with 20 paired samples at each payload. It is a proxy
screen of the signed Windows driver path, not an `atom_arc` native-driver test.

| Payload | Operations/sample | Pairs | Mean | Median | Lower 95% | Baseline p95 ns | Third-lane p95 ns | p95 change | Verdict |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 4 KiB | 64 | 20 | +30.816% | +31.191% | +25.422% | 4,592,406 | 3,549,529 | -22.709% | local retain |
| 64 KiB | 32 | 20 | +3.886% | +7.305% | -2.559% | 5,493,759 | 5,961,105 | +8.507% | reject |
| 1 MiB | 16 | 20 | +0.490% | +0.084% | -0.750% | 29,745,395 | 27,567,638 | -7.321% | reject |
| 16 MiB | 4 | 20 | +0.202% | +0.312% | -0.215% | 106,862,691 | 106,324,490 | -0.504% | reject |
| 256 MiB | 3 | 20 | -0.074% | +0.002% | -0.664% | 1,249,797,426 | 1,248,921,057 | -0.070% | reject |

All 100 paired correctness digests matched and every measurement completed
without an error. The candidate nevertheless failed four of five payload
classes, including a p95 latency regression at 64 KiB, so the executable third
lane was removed. The raw negative records remain under
`experiments/results/proxy-a380-third-lane-*.json`; a single small-payload win
does not authorize promotion of the compound stack.

### Phase 2: VRAM placement and lifetime

Apply the Aedes `acquire`/`release` primitive to driver-owned allocations.
Released blocks become deterministic first eviction candidates. Compare buddy,
slab/size-class, and traffic-formed hot pools. No silent system-memory spill.

### Phase 3: command highway

Compare single submission, batched doorbells, persistent rings, queue-depth
control, and backpressure-aware admission. Small irregular commands stay on the
ground path; stable flows may form promoted command lanes.

### Phase 4: engine overlap

Measure copy/compute concurrency, command preloading, fence granularity, and
engine-aware routing. Promotion requires reduced wall time, not merely higher
individual engine utilization.

### Phase 5: intra-GPU data hopping

Test data routes that avoid unnecessary round trips: copy-engine relays,
producer-to-consumer residency, alias-safe remapping, and cut-through tiles.
Each hop must be cheaper than the route it replaces after synchronization cost.

### Phase 6: cross-device fabric

Only after the A380 path is stable. Advertise capabilities and measure real
routes between the A380, RTX 5070 Ti, pinned RAM, and NVMe. The current VFIO
boundary does not provide A380-to-5070-Ti P2P, so no such lane may be claimed
until the topology actually exposes and verifies it.

## Aedes boundary

Aedes concepts govern memory lifetime and transfer staging: explicit release,
deterministic placement, reusable pinned buffers, capability advertising, and
telemetry. The driver remains responsible for PCI ownership, firmware
authentication, GPU address spaces, engines, interrupts, reset, and hardware
submission. Prefetch proposes candidates; the owning pool/driver decides which
transfers occur.
