# Borrow Strategy — Readonly BindSpace + Owned Microcopies

## The Pattern (from ladybug-rs, mandatory for lance-graph)

### Problem
Graph operations need to read the BindSpace/SpoStore/GrBMatrix while
simultaneously computing mutations. Rust's borrow checker rejects
`&self` reads mixed with `&mut self` writes in the same scope.

### Solution: Readonly BindSpace + Owned Microcopies

```rust
// The BindSpace / SpoStore / GrBMatrix is ALWAYS &self (readonly)
// NEVER hold &mut on the main structure during computation

// 1. READ: take a readonly reference
let result = bind_space.read(addr);          // &self
let hits = spo_store.query_forward(subject); // &self
let row = matrix.row(i);                     // &self

// 2. COMPUTE: work on owned microcopies
let mut local_truth = hit.record.truth;      // owned Copy
let mut local_fp = hit.record.subject;       // owned Copy, [u64; 8]
local_truth = local_truth.revision(&new_evidence);

// 3. WRITE BACK: gated, one of two patterns:

// Pattern A — Single writer: gated XOR
// Safe because XOR is idempotent — applying twice = no-op
bind_space.write_gated_xor(addr, local_fp);

// Pattern B — Multiple writers: BUNDLE (majority vote)
// Safe because bundle accumulates — order doesn't matter
bind_space.write_bundle(addr, &[local_fp_1, local_fp_2, local_fp_3]);
```

### Why This Works
- `read()` is `&self` — any number of concurrent readers
- Microcopies are `Copy` types (`TruthValue`, `Fingerprint = [u64; 8]`, `u64`)
  — cheap to clone, no allocation, no borrow
- Write-back is GATED:
  - XOR: idempotent, single writer, no race condition
  - BUNDLE: commutative + associative, multiple writers, order-independent
- No `&mut self` during computation → no borrow conflicts

### Race Condition Prevention

```
Single writer  → use gated XOR    (applying twice = identity)
Multiple writers → use BUNDLE      (majority vote, commutative)
NEVER use raw assignment (=) for write-back on shared state
```

## SIMD Exception: SLICING, NOT COPYING

SIMD operations work on SLICES of the backing store. Do NOT copy data
out for SIMD — that defeats the purpose. SIMD reads contiguous memory.

```rust
// CORRECT: slice into the backing store
let plane_bytes: &[u8] = node.plane_s.as_bytes();  // zero-copy slice
let distance = dispatch_hamming(plane_bytes, query_bytes);  // SIMD on slice

// WRONG: copy then SIMD
let owned = plane_bytes.to_vec();  // kills SIMD alignment + wastes memory
let distance = dispatch_hamming(&owned, query_bytes);
```

The rule:
- SIMD: `&[u8]` slices into the original data. Never owned copies.
- NARS/reasoning: owned `Copy` microcopies of small types. Never borrow.
- Write-back: gated XOR (single) or BUNDLE (multiple). Never raw `=`.

These three patterns coexist. SIMD reads the shared store via slices.
Reasoning reads the shared store, computes on owned microcopies, writes
back through gates. No conflicts.

## Severity

Changing SIMD to use owned copies is a P0 (kills performance).
Changing BindSpace to use `&mut self` during computation is a P0 (won't compile
in async/concurrent contexts, and is architecturally wrong).
Using raw assignment for write-back is a P0 (race conditions).
