//! WebMercator `(lon, lat)` → slippy tile `z/x/y` → HHTL key.
//!
//! A faithful port of the pure math in `crates/cockpit-server/src/osm_tiles.rs`
//! (kept dep-free here — no serde, no axum). A slippy-map quadtree IS an HHTL
//! cascade: `z/x/y` Morton-interleaves directly into the three 16-bit tiers
//! (`3 × 256×256 = 48 bits`), coarse zoom → HEEL, fine → TWIG, `tier = level >> 3`.
//! This is the spatial binding of the OGAR 256×256-centroid-tile cascade; the
//! same key a semantic node would carry, with the axes bound to literal x/y.

/// Native HHTL depth: 3 tiers × 8 quadtree levels each = 24 zoom levels.
pub const HHTL_DEPTH: u32 = 24;

/// The geo domain nibble — OSM concepts live at classid `0x0F0X` (`osm_node`
/// `0x0F01` … `osm_user` `0x0F0A`), so the classid high byte is `0x0F`.
pub const GEO_DOMAIN: u8 = 0x0F;

const PI: f64 = std::f64::consts::PI;

/// The three HHTL cascade tiers of a slippy tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hhtl {
    /// Coarsest tier (the low-zoom neighbourhood).
    pub heel: u16,
    /// Middle tier (the palette tier in the HHTL legend).
    pub hip: u16,
    /// Finest tier (the leaf tile).
    pub twig: u16,
}

/// WebMercator (EPSG:3857) forward: `(lon, lat)` → slippy tile `(x, y)` at zoom
/// `z`. `x` grows east, `y` grows south. Clamped to the valid `0..2^z` range.
#[must_use]
pub fn lonlat_to_tile(lon: f64, lat: f64, z: u32) -> (u32, u32) {
    let n = f64::from(2u32.saturating_pow(z));
    let x = ((lon + 180.0) / 360.0 * n).floor();
    let lat_rad = lat.to_radians();
    // asinh(tan φ) = ln(tan φ + sec φ) — the Mercator y, no PROJ needed.
    let merc_y = (lat_rad.tan() + 1.0 / lat_rad.cos()).ln();
    let y = ((1.0 - merc_y / PI) / 2.0 * n).floor();
    let max = (n - 1.0).max(0.0);
    (x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32)
}

/// Morton-interleave two 24-bit lanes into a 48-bit code: `x` bit `i` → code bit
/// `2i`, `y` bit `i` → code bit `2i+1`.
#[must_use]
pub fn morton_interleave(x24: u32, y24: u32) -> u64 {
    let mut code = 0u64;
    for i in 0..HHTL_DEPTH {
        code |= u64::from((x24 >> i) & 1) << (2 * i);
        code |= u64::from((y24 >> i) & 1) << (2 * i + 1);
    }
    code
}

/// The `z/x/y` the HHTL key actually encodes: identity for `z <= 24`, else the
/// `z=24` ancestor (excess low bits of `x`/`y` dropped, not kept in a low lane).
#[must_use]
pub fn resolved_tile(z: u32, x: u32, y: u32) -> (u32, u32, u32) {
    if z > HHTL_DEPTH {
        let excess = z - HHTL_DEPTH;
        (HHTL_DEPTH, x >> excess, y >> excess)
    } else {
        (z, x, y)
    }
}

/// Map a slippy tile `z/x/y` onto its HHTL key. The tile is left-aligned to the
/// native 24-level depth so the coarsest quadtree bit lands in HEEL's high bit
/// (`tier = level >> 3`); fine bits fall into TWIG. A zoom deeper than 24
/// resolves to its `z=24` ancestor (a correct prefix).
#[must_use]
pub fn tile_to_hhtl(z: u32, x: u32, y: u32) -> Hhtl {
    let (_, x, y) = resolved_tile(z, x, y);
    let z = z.min(HHTL_DEPTH);
    let shift = HHTL_DEPTH - z;
    let code = morton_interleave(x << shift, y << shift);
    Hhtl {
        heel: (code >> 32) as u16,
        hip: (code >> 16) as u16,
        twig: code as u16,
    }
}

/// The whole "where is this point, and what's its key" answer: a geographic
/// point at zoom `z` → its HHTL tier key. The spatial half of a node's GUID
/// (`classid 0x0F0X | HEEL | HIP | TWIG | …`); the value bits (feature tags,
/// geometry residue) are everything the key isn't.
#[must_use]
pub fn point_to_hhtl(lon: f64, lat: f64, z: u32) -> Hhtl {
    let z = z.min(HHTL_DEPTH);
    let (x, y) = lonlat_to_tile(lon, lat, z);
    tile_to_hhtl(z, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morton_roundtrips_the_bremen_tile() {
        // Bremen city centre ≈ (8.807, 53.075). At z=16 it must land in a stable
        // tile, and the Morton interleave of the left-aligned tile must be
        // recoverable tier-by-tier.
        let (x, y) = lonlat_to_tile(8.807, 53.075, 16);
        let hhtl = tile_to_hhtl(16, x, y);
        // Reassemble the 48-bit code from the three tiers and de-interleave.
        let code =
            (u64::from(hhtl.heel) << 32) | (u64::from(hhtl.hip) << 16) | u64::from(hhtl.twig);
        let (rx, ry) = {
            let (mut rx, mut ry) = (0u32, 0u32);
            for i in 0..HHTL_DEPTH {
                rx |= (((code >> (2 * i)) & 1) as u32) << i;
                ry |= (((code >> (2 * i + 1)) & 1) as u32) << i;
            }
            (rx, ry)
        };
        // Left-aligned by (24 - 16) = 8 bits.
        assert_eq!(rx >> 8, x, "x lane survives interleave");
        assert_eq!(ry >> 8, y, "y lane survives interleave");
    }

    #[test]
    fn deeper_zoom_shares_the_ancestor_key() {
        // Two z=25 children of one z=24 tile must collapse to the same HHTL key
        // (a valid prefix) — depth beyond 24 is the hierarchy's job.
        let a = tile_to_hhtl(25, 100, 200);
        let b = tile_to_hhtl(25, 101, 201);
        assert_eq!(a, b, "over-depth children share the z=24 ancestor key");
    }

    #[test]
    fn point_key_matches_tile_key() {
        let z = 14;
        let (x, y) = lonlat_to_tile(8.807, 53.075, z);
        assert_eq!(point_to_hhtl(8.807, 53.075, z), tile_to_hhtl(z, x, y));
    }
}
