//! Morton (Z-order) tile-pyramid codec — the universal `high:low = tile:cell`
//! address primitive.
//!
//! The convergence the addressing rides on: a node's position in any
//! container→unit hierarchy (galaxy:planet, country:city, organ:tissue,
//! cm²:mm², residue:atom) is a **Z-order quadtree path**. Interleave the x/y
//! bits 2-at-a-time (`2bit×2bit` → a 4×4 = 16-way branch per level) and the
//! address bijects to a 2-D tile pyramid:
//!
//! * **bijective** — every (x, y) has exactly one code, every code one (x, y).
//!   No overflow basin to collide (the class of bug that hit `ti=15`).
//! * **locality-preserving** — nearby codes are nearby in space, so a layout is
//!   a deinterleave, not a force pass.
//! * **hierarchical** — the high bits are the coarse tile (family), the low bits
//!   the fine cell (identity); truncating the code climbs the pyramid.
//!
//! Domain-agnostic: the FMA heart slice, OSINT basins, and a chess board all ride
//! the same codec; only what fills the tiles differs.

/// Spread the low 16 bits of `n` into the even bit positions (0,2,4,…) — the
/// "part-1-by-1" bit-interleave half (standard Morton magic-number cascade).
const fn part1by1(mut n: u32) -> u32 {
    n &= 0x0000_ffff;
    n = (n | (n << 8)) & 0x00ff_00ff;
    n = (n | (n << 4)) & 0x0f0f_0f0f;
    n = (n | (n << 2)) & 0x3333_3333;
    n = (n | (n << 1)) & 0x5555_5555;
    n
}

/// Gather the even bit positions of `n` back into the low 16 bits — the inverse
/// of [`part1by1`].
const fn compact1by1(mut n: u32) -> u32 {
    n &= 0x5555_5555;
    n = (n | (n >> 1)) & 0x3333_3333;
    n = (n | (n >> 2)) & 0x0f0f_0f0f;
    n = (n | (n >> 4)) & 0x00ff_00ff;
    n = (n | (n >> 8)) & 0x0000_ffff;
    n
}

/// Interleave `(x, y)` into a 32-bit Z-order (Morton) code: x in the even bits,
/// y in the odd bits. `2bit×2bit` per pyramid level falls out naturally — bits
/// `[2k, 2k+1]` are level *k*'s 4×4 cell.
pub const fn encode(x: u16, y: u16) -> u32 {
    part1by1(x as u32) | (part1by1(y as u32) << 1)
}

/// Deinterleave a Z-order code back to `(x, y)` — this is what `position()`
/// becomes: the address *is* the coordinate.
pub const fn decode(m: u32) -> (u16, u16) {
    (compact1by1(m) as u16, compact1by1(m >> 1) as u16)
}

/// Push one 4×4 level onto a Z-order path: `(tx, ty)` each in `0..4` (2 bits).
/// Descending the container→unit hierarchy one step = one `descend`, high bits
/// first, so the most-significant levels are the coarsest tiles (the family).
pub const fn descend(parent: u32, tx: u16, ty: u16) -> u32 {
    (parent << 4) | encode(tx & 0b11, ty & 0b11)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for &(x, y) in &[(0u16, 0u16), (1, 0), (0, 1), (3, 2), (255, 255), (40_000, 25_000)] {
            assert_eq!(decode(encode(x, y)), (x, y), "round-trip ({x},{y})");
        }
    }

    #[test]
    fn interleaves_2bit_by_2bit() {
        // x=0b01 (1), y=0b10 (2) → bits: x at 0,2 ; y at 1,3 → 0b1001 = 9.
        assert_eq!(encode(0b01, 0b10), 0b1001);
        // the 4×4 cell at level 0 is the low nibble; level 1 is the next nibble.
        assert_eq!(encode(3, 3) & 0b1111, 0b1111, "(3,3) fills one 4×4 cell");
    }

    #[test]
    fn descend_nests_tiles() {
        // a path: root → quadrant (1,1) → sub-cell (2,0). The high nibble is the
        // coarse tile (family), the low nibble the fine cell (identity).
        let coarse = descend(0, 1, 1);
        let fine = descend(coarse, 2, 0);
        assert_eq!(fine >> 4, coarse, "truncating climbs to the parent tile");
        // sibling cells under the same parent share the high bits.
        let sib = descend(coarse, 0, 3);
        assert_eq!(fine >> 4, sib >> 4, "siblings share the family prefix");
        assert_ne!(fine, sib, "but differ in the identity cell");
    }

    #[test]
    fn locality_preserving() {
        // horizontally-adjacent cells stay within one 4×4 block of each other.
        let a = encode(10, 10);
        let b = encode(11, 10);
        assert!((a as i64 - b as i64).abs() <= 16, "adjacent cells stay near in Z-order");
    }
}
