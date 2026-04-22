# Eliminate `serde_json::Value` intermediate in pampa JSON writer

Status: **investigation complete, fix plan drafted**

Beads: bd-wgup. Previous session on the same file established the
`SourceInfoSerializer` hotspot (bd-h5l7,
`claude-notes/plans/2026-04-22-sourceinfo-eq-hotspot.md`); this is a
follow-up identified during Phase E browser validation of that fix.

## Context

The hub-client's `parse_qmd_to_ast` (wraps
`quarto_core::pipeline::parse_qmd_to_ast` and serializes its output via
`pampa::writers::json::write_with_config` with `include_inline_locations:
true`) is the primary read path in the preview render loop. After the
bd-h5l7 fix, Chrome profiling showed the remaining time dominated by
serde_json-related symbols:

```
<serde_json::value::Value as serde_core::ser::Serialize>::serialize
<alloc::collections::btree::map::BTreeMap<String, Value>>::insert
```

This plan establishes a reproducible native measurement of the same
code path, identifies the specific pattern responsible, and proposes a
fix direction validated against data.

## Native measurement setup

A new `crates/perf-harness/` crate houses drivers for native profiling
of hub-client entry points. The first driver, `parse-qmd-to-ast`, mirrors
the exact call chain in `crates/wasm-quarto-hub-client/src/lib.rs:757`:

1. `quarto_core::pipeline::parse_qmd_to_ast` (Parse → EngineExecution →
   MetadataMerge stages)
2. Build an `ASTContext` from the returned `SourceContext`
3. `pampa::writers::json::write_with_config` with
   `JsonConfig { include_inline_locations: true }`

```bash
# Build with debuginfo preserved for samply/atos symbol resolution
cargo build --profile=release-perf -p perf-harness

# One iteration (for large fixtures):
target/release-perf/parse-qmd-to-ast /tmp/q2-intern-bench/8x.qmd 1

# Many iterations (for small fixtures, so samply has samples):
target/release-perf/parse-qmd-to-ast /tmp/q2-intern-bench/1x.qmd 30
```

A new `[profile.release-perf]` in the workspace root `Cargo.toml`
inherits from `release` but sets `debug = true, strip = false` so
samply can resolve Rust symbols. Don't profile against plain `--release`;
the default release profile strips debuginfo and you get a useless
forest of raw addresses.

### samply workflow

```bash
# Record with presymbolication so we get resolved symbols offline
samply record -s -n --unstable-presymbolicate \
  -o /tmp/q2-perf-profiles/parse-qmd-8x-sym.json.gz -- \
  target/release-perf/parse-qmd-to-ast /tmp/q2-intern-bench/8x.qmd 3

# Inspect top self-time via the analyzer checked into the repo.
crates/perf-harness/scripts/analyze_profile.py \
  /tmp/q2-perf-profiles/parse-qmd-8x-sym.json.gz --top 30
```

The analyzer (`crates/perf-harness/scripts/analyze_profile.py`) merges
the profile's `threads[].stringArray` entries that look like `"0x..."`
with the sidecar syms table (`{rva, size, symbol}` triples per module)
and reports self-time per resolved symbol. stdlib-only Python; its
head docstring documents both the profile and sidecar formats.

## Findings

### Wall time scales ~linearly in the driver

| Size | JSON bytes | user CPU |  ratio |
|------|-----------:|---------:|-------:|
| 1×   |  1,170,632 |   0.11 s |   —    |
| 2×   |  2,382,298 |   0.23 s |   2.09× |
| 4×   |  4,831,709 |   0.54 s |   2.35× |
| 8×   |  9,826,609 |   1.37 s |   2.54× |

Growth per doubling is ~2.1–2.5×. Slightly super-linear in the tail,
consistent with allocator/cache effects as the output buffer grows
past L2/L3 cache sizes. No quadratic pathology — the work is genuinely
linear in AST node count, just with a high constant factor.

### Self-time distribution (symbolicated)

Top symbols at each fixture size (self-time % from 3–4k sample counts):

| Symbol (self-time) | 1× | 2× | 4× | 8× |
|---|---:|---:|---:|---:|
| `_platform_memmove` | 11.2% | 18.5% | 27.3% | **41.1%** |
| `indexmap::Core<String, Value>::insert_full` | 7.3% | 6.7% | 5.1% | 5.3% |
| `_nanov2_free` | 5.8% | 6.2% | 4.8% | 4.5% |
| `_malloc_zone_malloc` | 3.9% | 3.6% | 3.0% | 2.2% |
| `nanov2_malloc_type` | 3.2% | 3.3% | 3.1% | 2.7% |
| `tiny_malloc_from_free_list` | 3.3% | — | — | 2.3% |
| `__vfprintf` (format) | 3.8% | 3.8% | 3.6% | 2.3% |
| `hashbrown::entry` (indexmap) | 3.1% | — | — | 1.8% |
| `serde_json::Value::serialize` | 2.8% | — | 2.9% | 1.6% |
| `RandomState::hash_one::<&String>` | 2.7% | — | — | 1.5% |
| `Bucket<String, Value>::clone` | — | — | — | 0.7% |

Observations:

1. **`_platform_memmove` share rises with document size** — 11% → 41%
   across 1× → 8×. Pure memory copying. Something is moving increasing
   volumes of bytes per AST node as the document grows; constant
   overhead hypothesis is rejected.
2. **Allocator churn is a flat ~13–15% tax** regardless of size. Many
   small allocations (Values, Strings, IndexMap buckets) at a steady
   per-node rate.
3. **`indexmap::Core::insert_full` is ~5–7% on its own.** Every AST
   node builds a `Value::Object` backed by `IndexMap<String, Value>`;
   each insert hashes and inserts its key.
4. **Tree-sitter parsing is <2% at every scale.** The parser is not
   the bottleneck.
5. **`__vfprintf` at ~3%** — likely number formatting inside the
   serde_json output path (f64 / integer → decimal text). Not dominant
   but visible.

### Root cause

`pampa::writers::json::write_with_config` operates in two passes:

1. **Build pass**: constructs a `serde_json::Value` tree. Every AST
   node becomes a `Value::Object` (IndexMap<String, Value>) with
   freshly-allocated `String` keys (`"c"`, `"s"`, `"t"`, `"attrS"`,
   `"targetS"`, etc.) and `Value::*` leaves. For a document producing
   ~10 MB of JSON, this tree is on the order of tens of megabytes in
   memory, distributed across tens of thousands of allocations.

2. **Serialize pass**: `serde_json::to_writer(&mut buf, &value)` walks
   the tree and emits UTF-8 bytes to `Vec<u8>`, doubling the buffer on
   growth.

Both passes touch every byte. The build pass allocates and copies
string keys and IndexMap bucket contents; the serialize pass reads
every Value and memcpys its bytes into the output buffer; the output
buffer itself doubles in capacity as it grows past thresholds,
memcpying its entire current contents each time. Large documents
amortize this cost across a larger working set with poorer cache
locality — which is why `_platform_memmove`'s *fraction* climbs with
size.

### Why we do this today

The Value-tree pattern comes from the original writer design:
`write_pandoc` returns a `Value` which the top-level writer then
serializes. The intermediate form made it easy to rearrange field
order before output and compose pieces across helpers. It was never
chosen for performance reasons.

## Fix direction

**Primary (F1) — Stream JSON directly, skip the Value intermediate.**

Replace the `write_pandoc() -> Value` + `serde_json::to_writer(value)`
pattern with a direct serializer that writes UTF-8 to a `&mut dyn
io::Write` as it walks the Pandoc AST. Two implementation strategies:

1. **Custom `Serialize` impls on AST types.** Each `Inline`/`Block` gets
   a hand-written `Serialize` that calls `serializer.serialize_struct`
   / `serializer.serialize_map` with `&'static str` field names. No
   IndexMap, no String key allocations, no intermediate Value. Plays
   nicely with `serde_json::Serializer<&mut Vec<u8>>`.
2. **Direct byte writer.** Skip serde entirely. Write JSON bytes with a
   small writer (`write_all(b"{")`, escape helpers, number formatting
   via `itoa`/`ryu`). Maximum control, but reimplements JSON rules
   (escaping, number formatting, field ordering).

Recommend starting with (1). It preserves serde's correctness
guarantees around escaping and number formatting, drops the dominant
cost centers (Value tree alloc + IndexMap inserts + String key
allocs), and is straightforwardly verifiable against the existing
snapshot tests. (2) is a larger change with independent bugs; use it
only if (1) doesn't move the needle enough.

A known wrinkle: the current writer requires **deterministic field
ordering** (`c`, `s`, `t` alphabetical per comment in `json.rs:34`).
This is easy with a hand-written `Serialize` — you control the order
you emit fields. It's harder if you use `#[derive(Serialize)]` on
structs because serde serializes in declaration order, which forces
the struct fields themselves to be ordered. We can keep declaration
ordering or provide explicit serialize impls; both work.

**Secondary — interning the field-name strings.**

Even inside the current Value-tree pattern, replacing `String::from("c")`
/ `"c".to_string()` with `&'static str` where possible would cut
string allocations. But if F1 lands, this mostly goes away — direct
serializers emit `&'static str` keys naturally.

**Tertiary — investigate `__vfprintf` (~3%).**

Likely f64 number formatting inside serde_json's `fmt::Display for
Number`. Could be swapped for `ryu` if it's not already. Only worth
looking at if F1 leaves this as a relevant fraction.

## Validation plan

Each phase re-uses the perf-harness driver and the samply workflow.

### Phase 1 — Build baseline, then implement F1

- [ ] Re-verify the baseline reproduces: `samply record -s -n
      --unstable-presymbolicate` on 1× through 8×, confirm the
      `_platform_memmove` share matches the table above (±2%).
- [ ] Choose an incremental strategy for the `Serialize` conversion.
      Two realistic starting points:
       - Top-down from `write_pandoc`: emit the outer document
         structure as a single hand-written `Serialize` that calls
         into inner writers.
       - Bottom-up from `Inline`: convert the leaf types first, let
         the outer still build a Value for the parts not yet
         converted, and watch the profile shift as conversions move
         up.
      Prefer bottom-up — `Inline` dominates node count, so the biggest
      wins come earliest.
- [ ] Add a criterion-style bench (or keep using the driver) that
      produces before/after numbers on the same fixtures.

### Phase 2 — Incremental conversion

- [ ] Convert `Inline` variants to direct `Serialize` impls. Verify
      snapshot equivalence after each batch of related variants (see
      below).
- [ ] Convert `Block` variants.
- [ ] Convert `ConfigValue` / `Meta` variants.
- [ ] Convert the outer `Pandoc` + `ASTContext` envelope.

### Phase 3 — Snapshot canonicalization check

The snapshot test infrastructure compares exact JSON byte strings. If
we change field ordering or add/remove whitespace, all snapshots will
churn. Use the same approach as bd-h5l7:

- [ ] Write a Python canonicalizer that parses JSON + resolves pool
      references, and use it to verify structural equivalence before
      accepting snapshot diffs. The one at
      `/tmp/check_snap_diffs2.py` from the previous session is a
      starting template.
- [ ] Field ordering should stay identical (alphabetical) throughout
      the migration to minimize churn. If necessary, make field
      ordering a verified contract with a doc comment.

### Phase 4 — Re-profile

- [ ] After each phase, re-run `samply record` on 1× / 4× / 8× and
      record the new top symbols. Expected: `_platform_memmove`
      fraction drops substantially, `indexmap::insert_full` should
      disappear entirely, allocator churn drops, and total wall time
      should shrink noticeably.
- [ ] Record the before/after table in this plan as Findings for
      future reference.

### Phase 5 — Full verification

- [ ] `cargo nextest run --workspace` clean.
- [ ] `cargo xtask verify` clean (hub-client WASM + vitest).
- [ ] Hub-client browser cross-validation: repeat Carlos's Chrome
      profile on the canonical `test.qmd` and confirm serde_json
      symbols are no longer near the top.

## Reproducing this investigation

The raw samply profiles used above live in `/tmp/q2-perf-profiles/`
and the analyzer at `/tmp/analyze_profile2.py` — both are tmp and
won't survive a reboot. Reproduction recipe:

```bash
# 1. Build scaled fixtures from the canonical 50-paragraph lorem ipsum.
#    (Created during the bd-h5l7 session.)
mkdir -p /tmp/q2-intern-bench
cp /Users/cscheid/Desktop/daily-log/2026/04/22/test.qmd /tmp/q2-intern-bench/1x.qmd
for n in 2 4 8 16; do
  python3 -c "import sys; sys.stdout.write(open('/tmp/q2-intern-bench/1x.qmd').read() * $n)" \
    > /tmp/q2-intern-bench/${n}x.qmd
done

# 2. Build the driver with debuginfo
cargo build --profile=release-perf -p perf-harness

# 3. Record profiles with presymbolication
mkdir -p /tmp/q2-perf-profiles
for spec in "1 30" "2 15" "4 6" "8 3"; do
  read n iter <<< "$spec"
  samply record -s -n --unstable-presymbolicate \
    -o /tmp/q2-perf-profiles/parse-qmd-${n}x-sym.json.gz -- \
    target/release-perf/parse-qmd-to-ast /tmp/q2-intern-bench/${n}x.qmd $iter
done

# 4. Analyze — produces the tables above
for n in 1 2 4 8; do
  echo "=== ${n}x ==="
  crates/perf-harness/scripts/analyze_profile.py \
    /tmp/q2-perf-profiles/parse-qmd-${n}x-sym.json.gz --top 15
done
```

## Open questions

1. Is the field-ordering contract (alphabetical) a hard requirement
   of any downstream consumer? Or can we pick any stable order? Worth
   checking the TypeScript side of hub-client before committing to an
   ordering.
2. Should `ASTContext.sourceInfoPool` emit as an array (current) or
   move to a stream-friendlier format (e.g. newline-delimited) if we
   streaming-write? Probably no — stays an array, but flag it.
3. Is anyone depending on exact whitespace in the output JSON? Insta
   snapshots will tell us the first time we run.
