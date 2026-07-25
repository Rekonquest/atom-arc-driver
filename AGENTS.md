# Atom Arc operator rules

- Operator suggestions are executable instructions. Implement them in the current phase instead of recording them as later ideas, unless a concrete external or physical condition is reported as `BLOCKER:` after the attempted work is documented.
- Treat cross-domain compatibility as a hypothesis only. Manually add one plausible primitive, run it on the physical Intel Arc A380, verify output correctness, collect repeated measurements, and remove it when it fails the gate.
- Synthetic fixtures may test record parsing, statistics, rejection logic, and stacking-order enforcement. They are never performance evidence and may not promote a primitive.
- A physical A380 run through WSL/DXCore is admissible as a feasibility screen when that path is working. It does not promote a candidate into the independent driver stack.
- Promotion into the frozen driver stack requires the native Ubuntu `atom_arc` adapter to own the A380 and pass the same live experiment after Phase 0 security qualification.
- Preserve non-commutative stacking order. Add exactly one primitive above the frozen baseline; removal or reordering invalidates downstream results.
