# Lua attribute-mutation proxy (bd-195t)

## Problem

Quarto 2's Lua bridge returns **fresh copies** of element `attr` and its
`.attributes` / `.classes` fields on every read. As a consequence, the
idiomatic Pandoc-Lua pattern

```lua
function CodeBlock(cb)
  cb.attr.attributes["data-hl-spans"] = my_encoding
  return cb
end
```

silently does nothing: the write lands on an ephemeral Lua table that is
discarded the moment the filter returns. The AST that comes out of the
filter has no `data-hl-spans` set. No error, no warning.

This was uncovered while building the Phase 3.5 filter-authored-spans
fixture (`crates/quarto/tests/smoke-all/highlighting/04-filter/`), where
we worked around it by rebuilding the whole `Attr` and reassigning:

```lua
local attrs = cb.attr.attributes
attrs["data-hl-spans"] = pandoc.json.encode(spans)
cb.attr = pandoc.Attr(cb.attr.identifier, cb.attr.classes, attrs)
return cb
```

That workaround is acceptable for an internal test fixture but is a
usability regression compared to Pandoc's Lua API — and we don't want
it to be the shape of the Lua filter examples we ship with the syntax
highlighting docs. Before we encourage filter-authored highlighting as
a user-facing feature, idiomatic attribute mutation must persist.

The follow-up pointer is in
`claude-notes/plans/2026-04-20-syntax-highlighting-phase-3.5.md`
("Follow-up task: Lua attribute-mutation proxy").

## Root cause (code references)

- `crates/pampa/src/lua/types.rs:1733` —
  `attr_to_lua_userdata` creates `LuaAttr::new(attr.clone())`. The
  `LuaAttr` is disconnected from the parent block/inline.
- `crates/pampa/src/lua/types.rs:1591-1596` — `LuaAttr::get_field`
  returns a fresh Lua table populated from the cloned attributes map
  on each access to `.attributes`. No write-back.
- Similarly `crates/pampa/src/lua/types.rs:1570-1583` — positional and
  `.classes` accessors return fresh tables.
- `crates/pampa/src/lua/types.rs:662` (block) and `:120` (inline) —
  the `cb.attr` / `code.attr` read returns a fresh `LuaAttr` userdata
  (via `attr_to_lua_table` → `attr_to_lua_userdata`).

The chain `cb.attr.attributes["k"] = v` thus produces three
disconnected values, none of which route back to the original block.

## Design options

### A. Shared interior mutability — `Rc<RefCell<...>>` (preferred)

Store the AST nodes behind `Rc<RefCell<...>>` inside the Lua userdata
wrappers:

```rust
pub struct LuaBlock(pub Rc<RefCell<Block>>);
pub struct LuaInline(pub Rc<RefCell<Inline>>);
```

Then accessing `cb.attr` returns a *proxy* userdata that shares the
same `Rc` and knows how to reach the `Attr` inside. Accessing
`.attributes` returns a *proxy* userdata that shares the same `Rc` and
routes writes back into `attr.2`. Likewise for `.classes`.

We add a new userdata type (`LuaAttrView` / `LuaAttrProxy`, name TBD)
that carries:

```rust
enum LuaAttr {
    /// Standalone Attr (e.g. built via `pandoc.Attr(...)`). Mutations
    /// stay local until explicitly assigned back to an element.
    Owned(RefCell<crate::pandoc::Attr>),
    /// Proxy into a block's attr. Mutations are visible on the block.
    BlockRef(Rc<RefCell<Block>>),
    /// Proxy into an inline's attr.
    InlineRef(Rc<RefCell<Inline>>),
}
```

Similarly `LuaAttributesProxy` and `LuaClassesProxy` carry the shared
`Rc` and whether to look in `Block` or `Inline`.

**Pros**

- Matches Pandoc's API. `cb.attr.attributes["k"] = v`,
  `cb.attr.classes[#cb.attr.classes+1] = "warn"`,
  `cb.attributes["k"] = v` (the shortcut) — all persist.
- Aliases within a filter (`local a = cb.attr` then mutating `a`)
  behave as users expect.
- No behavioural surprises at the walker boundary: the walker still
  clones out an owned `Block`/`Inline` on `FromLua`, so mutations are
  scoped to the filter invocation — the same contract we have today.

**Cons**

- Touches `LuaBlock`/`LuaInline` internals. ~60 `LuaBlock(...)` and
  ~86 `LuaInline(...)` constructor sites in `crates/pampa/src/`. Most
  are in `types.rs` itself and the pattern is mechanical
  (`LuaBlock(b)` → `LuaBlock(Rc::new(RefCell::new(b)))`). The
  `FromLua` impls already clone — switching them to clone the inner
  value out of the cell preserves current semantics.
- New code is required to implement the three proxy userdata types
  with the expected `__index`/`__newindex`/`__pairs`/`__len`/`__ipairs`
  metamethods.

### B. Convenience methods only (`cb:set_attribute(k, v)`)

Add `cb:set_attribute("k", "v")`, `cb:set_class(i, name)`, etc. Keep
the current read-returns-copy semantics and document them.

**Pros**: small, surgical, no userdata plumbing.

**Cons**: diverges from Pandoc's API. Anyone copying an `elem.attributes["loading"] = "lazy"` snippet from Pandoc docs into a
Quarto filter will hit the same silent-drop bug we're trying to fix.
Helpful as a *complement* to A, but not a substitute.

### C. Commit-on-return or finalizer-based writeback

Rejected. Relying on Lua GC for correctness is fragile; the write
timing would be non-obvious.

**Decision: option A.** B's helper methods can be added on top for the
"I know exactly what I want to set, give me the short form" case, but
the core pattern must work because that's what users will copy from
Pandoc's docs.

## Plan (TDD)

### Phase 1 — Failing test

Before any implementation, write a Lua filter test that exercises the
idiomatic pattern and confirm it fails in the expected way.

- [x] **1.1** `crates/pampa/tests/test_lua_attr_mutation.rs ::
  test_cb_attr_attributes_nested_write_persists` — asserts that
  `cb.attr.attributes["data-hl-spans"] = v` persists onto `CodeBlock.attr.2`.
- [x] **1.2** `test_cb_attributes_shortcut_write_persists` — asserts
  that the block-level `cb.attributes[k] = v` shortcut persists.
- [x] **1.3** `test_cb_attr_classes_append_persists` — asserts
  `cb.attr.classes[#cb.attr.classes+1] = "warn"` persists.
- [x] **1.4** `test_inline_code_attr_attributes_write_persists` —
  asserts inline `code.attr.attributes[k] = v` persists.
- [x] **1.5** `test_pandoc_attr_owned_semantics` — exercises the
  Owned variant end-to-end: mutate a standalone `pandoc.Attr(...)`
  with `a.attributes[k] = v`, then assign to `cb.attr`, then mutate
  through `cb.attr.*` after assignment. All three mutations must
  land on the block.
- [x] **1.6** Ran all 5 tests pre-refactor. All 5 fail as expected:
  - 1.1 / 1.4 / 1.5: silent drop — `attrs = {}` in the output.
  - 1.2: `cb.attributes` is not a field at the block level today
    (Phase 5 adds the shortcut). Error: *attempt to index a nil
    value (field 'attributes')*.
  - 1.3: silent drop — classes list unchanged.
  Reality check: test 1.5 intentionally failed pre-refactor because
  the standalone-Attr write (`a.attributes[...] = ...`) also hits the
  same ephemeral-table bug today. Post-refactor it will pass because
  the Owned variant will also have a proxy write-path.

### Phase 2 — Refactor `LuaBlock` / `LuaInline` to shared-cell storage

- [x] **2.1** `LuaInline(pub Rc<RefCell<Inline>>)` and
  `LuaBlock(pub Rc<RefCell<Block>>)` with `::new(..)` constructors
  and `borrow_inline/borrow_block`, `clone_inline/clone_block`
  accessors.
- [x] **2.2** All constructor call sites migrated to `::new(..)` via
  targeted replace_all across `types.rs`, `constructors.rs`,
  `filter.rs`, `list.rs`, `shortcode.rs`, `diagnostics.rs`,
  `utils.rs`.
- [x] **2.3** `FromLua` for both types: `LuaInline::new(ud.borrow_inline().clone())`
  — deep-clones out of the source cell into a fresh cell,
  preserving per-invocation independence at filter boundaries.
- [x] **2.4** `get_field` borrows through the cell. Special-cased
  `tag`/`t`/`clone`/`walk` *before* the borrow (so closures don't
  capture a `Ref` lifetime). The main match is
  `match (&*inner, key)`. `set_field` takes `&self` and uses
  `self.0.borrow_mut()`.
- [x] **2.5** `cb:clone()` snapshots the inner value at
  `.clone`-field-access time (matching today's behavior), then each
  call of the returned function deep-clones that snapshot into a
  fresh `Rc`. `__pairs` captures a snapshot too.
- [x] **2.6** `cargo nextest run -p pampa --features lua-filter
  --no-fail-fast`: 3717 passed, 5 failed. The 5 failures are the
  Phase 1 tests that are *supposed* to fail until Phases 3–5 land.
  All pre-existing pampa tests still pass. `cargo build --workspace`
  succeeds — no external caller regressions.
- [x] **2.7** Secondary behavioral change: `__newindex` switched from
  `add_meta_method_mut` to `add_meta_method` (interior mutability now
  comes from the `RefCell`).

### Phase 3 — Proxy userdata: `LuaAttr` variants

- [x] **3.1** `LuaAttr` is now an enum with three variants —
  `Owned(Rc<RefCell<Attr>>)`, `BlockRef(Rc<RefCell<Block>>)`,
  `InlineRef(Rc<RefCell<Inline>>)`. `with_attr` / `with_attr_mut`
  helpers route reads/writes through the active variant via four
  new helpers: `block_attr_ref`, `block_attr_mut`,
  `inline_attr_ref`, `inline_attr_mut`.
- [x] **3.2** Added `attr_to_lua_userdata_for_block` /
  `attr_to_lua_userdata_for_inline` helpers alongside the existing
  `attr_to_lua_userdata` (now explicitly documented as the Owned
  path — used by `pandoc.Attr(...)` and by table-row-like wrappers
  that produce detached snapshots).
- [x] **3.3** All 13 `attr_to_lua_table(lua, &x.attr)` call sites
  in `get_field` for block/inline variants now route through the
  proxy helpers, passing `Rc::clone(&self.0)` for the parent cell.
  The dead `attr_to_lua_table` wrapper is removed.
- [x] **3.4** `set_field` on `"attr"` (block/inline) already went
  through `lua_value_to_attr(val, lua)`, which was updated to call
  `lua_attr.clone_attr()` (works across all enum variants) — so
  assigning a `BlockRef` or `InlineRef` proxy to another element's
  `.attr` copies the target's current Attr value in, correctly
  detaching it from its source cell.
- [x] **3.5** Same machinery (via `clone_attr`). No cross-cell
  aliasing is possible because every assignment copies the value
  through an owned `Attr`.
- [x] **3.6** `LuaAttr::get_field` still returns *fresh Lua tables*
  for `.attributes` and `.classes`. That deliberate decision isolates
  Phase 4 (proxy userdata for those tables) from the Phase 3 enum
  migration. Phase 3 tests therefore still fail — Phase 4 closes the
  gap.
- [x] **3.7** Build: `cargo build -p pampa --features lua-filter`
  succeeds. Tests: 3717 pass, 5 fail (the phase-1 regression
  targets). No new test regressions from Phase 3.

### Phase 4 — Proxy userdata: attributes + classes tables

- [ ] **4.1** Define `LuaAttributesProxy { parent: ParentRef }` and
  `LuaClassesProxy { parent: ParentRef }` userdata, where
  `ParentRef = Rc<RefCell<Block>> | Rc<RefCell<Inline>> | Rc<RefCell<OwnedAttr>>`.
- [ ] **4.2** Implement metamethods on `LuaAttributesProxy`:
  `__index` (read through to the map), `__newindex` (write through),
  `__pairs` (iterate the map), `__len` (count entries),
  `__tostring`.
- [ ] **4.3** Implement metamethods on `LuaClassesProxy`:
  `__index` with integer + string keys, `__newindex`,
  `__ipairs`/`__pairs`, `__len`, `__tostring`.
- [ ] **4.4** Wire `.attributes` and `.classes` reads on `LuaAttr`
  to return proxies (rather than fresh tables).
- [ ] **4.5** Keep whole-table assignment working: `cb.attr.attributes = {…}`
  replaces the whole map; `cb.attr.classes = {…}` replaces the whole
  list. These already work via `set_field` — adapt the proxy path to
  accept table RHS.

### Phase 5 — Block/inline shortcuts

Pandoc exposes `cb.attributes`, `cb.classes`, `cb.identifier` as
shortcuts for `cb.attr.2`, `cb.attr.1`, `cb.attr.0`. Block level
currently has `classes` and `identifier` but not `attributes`.

- [ ] **5.1** Add `"attributes"` to the `field_names()` list for
  every attr-bearing block and inline variant.
- [ ] **5.2** Add `"attributes"` read branch in `get_field` for each
  attr-bearing variant — returns a `LuaAttributesProxy` pointing at
  the element's attr.
- [ ] **5.3** Add `"attributes"` write branch in `set_field` for each
  attr-bearing variant.
- [ ] **5.4** (Audit only) Confirm `identifier` and `classes` writes
  correctly route through `set_field` with the new proxy — they
  already use raw `String::from_lua` / `lua_table_to_strings`, so
  the existing code should work unchanged.

### Phase 6 — Verify failing tests now pass

- [ ] **6.1** Re-run the tests from Phase 1. They must all pass.
- [ ] **6.2** Run `cargo nextest run -p pampa` — no regressions in
  existing filter tests (including deep-copy semantics, walking,
  alias behaviour).
- [ ] **6.3** Run `cargo nextest run --workspace` — no regressions
  downstream.

### Phase 7 — Update the 04-filter fixture

- [ ] **7.1** Rewrite `highlight-words.lua` to use the idiomatic
  pattern:
  ```lua
  cb.attr.attributes["data-hl-spans"] = pandoc.json.encode(spans)
  return cb
  ```
  Remove the note about the workaround.
- [ ] **7.2** End-to-end verify per CLAUDE.md "End-to-end
  verification": run
  `cargo run --bin quarto -- render crates/quarto/tests/smoke-all/highlighting/04-filter/04-filter-authored-spans.qmd`
  and inspect the rendered HTML for the expected `<span class="hl-error">` markup.
- [ ] **7.3** Re-run the smoke-all harness that covers the 04-filter
  fixture to confirm the test still passes with the new filter body.

### Phase 8 — User-facing examples

The whole point of the fix: being able to show filter-authored
highlighting in the docs.

- [ ] **8.1** Add at least one example fixture under
  `crates/quarto/tests/smoke-all/highlighting/` demonstrating a
  non-trivial custom highlighting filter (TBD — candidate: highlight
  structured log lines by severity and by timestamp pattern). Pattern:
  idiomatic `cb.attr.attributes["data-hl-spans"] = ...` with no
  workaround.
- [ ] **8.2** Add a `docs/` page under the syntax-highlighting
  section describing the Lua filter path with a copy-pasteable
  example.

### Phase 9 — Cross-platform verification + verification harness

- [ ] **9.1** `cargo xtask verify` — full Rust + hub-client + WASM
  chain. `LuaAttr` is a pampa type; hub-client's WASM build uses
  pampa, so the refactor must keep it green.
- [ ] **9.2** Document completion with the end-to-end snippet per
  CLAUDE.md section "End-to-end verification before declaring
  success".

## Design decisions (confirmed 2026-04-21)

1. **Naming.** Keep the name `LuaAttr` and make it an enum. The vast
   majority of call sites reference it abstractly, and the Lua-side
   userdata type-check is unchanged.
2. **Scope of Rc.** Only `LuaBlock` and `LuaInline` move to
   `Rc<RefCell<…>>`. `LuaAttr` carries either its own owned data or
   a handle back to the block/inline cell. Other Lua-exposed types
   (`LuaMeta`, citations, captions, etc.) are untouched. ~150
   constructor sites affected, mostly mechanical.
3. **Ordering of phases 3 & 4.** Phase 3 first. Phase 4 without 3
   means `cb.attr.attributes[k]=v` still doesn't persist because
   `cb.attr` remains a fresh copy.
4. **Thread safety.** `Rc`/`RefCell` are `!Send`/`!Sync`. Lua state
   runs single-threaded inside a filter invocation (walker is async
   but non-parallel per element; mlua's `Lua` is `!Send` on most
   configurations). If we ever want parallel filtering, switch to
   `Arc<Mutex<…>>`. Out of scope here; flagged as future work.
5. **`LuaAttr` Owned-vs-proxy semantics.** `pandoc.Attr(id, classes, attrs)`
   produces `LuaAttr::Owned(...)`. Mutations on the Owned variant
   don't propagate — correct, because the Attr isn't attached to any
   element. The `cb.attr = a` assignment copies the Owned Attr's
   value into the block's cell. Phase 1.5 locks this down as a test.
6. **Detached-proxy semantics.** If a filter stashes
   `cb.attr.attributes` in a global during one invocation and
   mutates it during a later (different-element) invocation, the
   write lands on a cell the walker already cloned out of — no
   effect on the AST. Document this ("you can't save an element
   across invocations"); don't try to detect via `__gc`. Same rule
   Pandoc effectively enforces.
7. **Docs/examples ordering.** Lua-filter highlighting examples
   (Phase 8) land *after* the proxy fix, so the shipped examples use
   idiomatic code from day one. No interim "known quirk" callout.

## Non-goals

- Generalizing proxy mutation to arbitrary fields (e.g. `cb.content[1] = x`
  modifying the content in place). That's a larger design question and
  is out of scope; today users either read the whole `content` table,
  mutate the table, and reassign, or use `cb:walk(...)`. We're only
  fixing the Attr path because that's what blocks the highlighting
  filter docs.
- Changing how filters are composed or how the walker traverses the
  AST.
- Adding new Lua APIs beyond proxy-enabled reads of existing fields
  (and the `attributes` shortcut at block level that Pandoc provides).

## References

- `claude-notes/plans/2026-04-20-syntax-highlighting-phase-3.5.md` —
  "Follow-up task: Lua attribute-mutation proxy"
- `crates/pampa/src/lua/types.rs:1566-1646` — the read/write path that
  currently returns fresh copies.
- `crates/pampa/resources/lua-types/pandoc/global.lua:17` — the
  `elem.attributes["loading"] = "lazy"` idiom we want to support.
- `crates/quarto/tests/smoke-all/highlighting/04-filter/highlight-words.lua:47-57` —
  the workaround we want to remove.
- Pandoc's native Lua proxy mechanism (for reference): every Lua
  element that wraps an AST node installs `__newindex` on its
  attribute children so writes propagate back. This plan mirrors
  that approach.
