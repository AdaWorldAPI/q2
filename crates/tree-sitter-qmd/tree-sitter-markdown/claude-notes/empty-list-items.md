# Empty List Items Investigation

## Problem
We want to support empty list items like:
```
* a
*
```
(where the second item has no content)

## Current State (RESOLVED)
- Test 11 (`* a\n* ` with EOF) PASSES
- Test 12 (`* a\n* \n\n`) PASSES

## Root Cause Analysis

After `list_marker_star` is emitted by the scanner, the cursor is at the end of the
marker (e.g., after `* `). For an empty list item, the next character is `\n`.

**Key constraint:** The scanner can only emit `_block_close` during `STATE_MATCHING`
(after a line ending, when trying to match open blocks) or at EOF. It CANNOT emit
`_block_close` mid-line. So the grammar must consume the `\n` before `_block_close`
can appear.

### Why the user's approach failed

The user tried adding `optional(seq($._blank_line, $._blank_line, ...))` as a choice
alongside `_list_item_content`. This failed because:

1. After `list_marker_star`, the scanner sees `\n` and emits `_line_ending` (at the
   LALR state after reduction, `_blank_line_start` was not valid).
2. The `_line_ending` gets consumed as part of `_newline` inside
   `_list_item_content → repeat1($._block) → _block_not_section → _newline`.
3. After `_line_ending`, when `_block_close` arrives (from the scanner's
   STATE_MATCHING phase), the parser cannot reduce `_newline` because `_block_close`
   was NOT in the LALR reduce action's lookahead set at that state.
4. This is due to LALR state merging: `_newline` is used in many grammar contexts
   (paragraphs, blank lines, standalone blocks, etc.), and the merged state didn't
   include `_block_close` as a valid reduce lookahead.

### Why a separate `_list_item_empty_tail` rule also failed

Creating a dedicated rule `_list_item_empty_tail: seq(_line_ending, optional(block_continuation))`
caused an explicit reduce/reduce conflict with `_newline` (same structure). Even with
a `conflicts` declaration for GLR forking, the parser still errored because both rules
have identical structure and tree-sitter merged their LALR states.

## Solution

Use `optional($._blank_line)` in the empty branch:

```javascript
_list_item_star: $ => choice(
    // Normal case: list item with content
    seq(
        $.list_marker_star,
        optional($.block_continuation),
        $._list_item_content,
        $._block_close,
        optional($.block_continuation)
    ),
    // Empty case: list item with no content
    seq(
        $.list_marker_star,
        optional($._blank_line),
        $._block_close,
        optional($.block_continuation)
    ),
),
```

### Why this works

1. `_blank_line` is `seq($._blank_line_start, choice($._newline, $._eof))`.
2. When the scanner sees `\n` after the list marker, it emits `_blank_line_start`
   (0-width token), which starts the `_blank_line` rule.
3. Inside `_blank_line`, `_newline` consumes the actual `\n` via `_line_ending`.
4. After `_newline` completes inside `_blank_line`, the `_blank_line` rule completes.
5. Then `_block_close` is expected — and the scanner emits it during STATE_MATCHING
   on the next line.

The key insight: `_blank_line` already has `_block_close` in its FOLLOW set from
other grammar contexts (e.g., `_list_item_content`'s two-blank-line termination
pattern uses `seq(_blank_line, _blank_line, ...)`). So `_block_close` is a valid
reduce lookahead at the state after `_blank_line` completes. No LALR conflicts arise.

### Why `optional(_blank_line)` handles the EOF case

When the scanner sees EOF directly after the list marker (test 11), it emits
`_block_close` without `_blank_line_start`. The `optional(_blank_line)` matches
nothing, and `_block_close` is consumed directly.

## Failed Approaches

1. **`optional(seq(_blank_line, _blank_line, _close_block, ...))`** — Requires TWO
   blank lines; empty items only have one.
2. **`_list_item_empty_tail: seq(_line_ending, optional(block_continuation))`** —
   Reduce/reduce conflict with `_newline` (identical structure).
3. **Conflict declaration `[$._list_item_empty_tail, $._newline]`** — GLR forking
   didn't resolve the issue; `_block_close` still wasn't valid at the merged state.
4. **`prec.left`/`prec.right` on `_list_item_content`** — Doesn't address the
   fundamental LALR state merging issue for `_newline`.
