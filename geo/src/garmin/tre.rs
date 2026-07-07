//! The TRE subfile: map bounding box, the LOD level pyramid, and the
//! subdivision tree. A direct port of `parse_tre` in `scripts/garmin_proto.py`.
//!
//! Layout: bbox N/E/S/W as signed int24 mapunits at 0x15/0x18/0x1B/0x1E;
//! the level table at `u32@0x21` (4 bytes each: `zoom | bit7 inherited`, bits,
//! `nsubdiv u16`); the subdivision table at `u32@0x29` — 16-byte records, with
//! the most-detailed (last) level using 14-byte records that omit `next`.

use super::{i24le, u16le, u24le, u32le, GarminError};

/// One LOD level of the map (coarsest first). `bits` is the coordinate
/// resolution used to shift deltas back to full mapunits in the RGN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub zoom: u8,
    pub inherited: bool,
    pub bits: u8,
    pub nsubdiv: u16,
}

/// One subdivision: a spatial cell owning a span of RGN data, tagged with which
/// object kinds it contains (`objtypes`: 0x10 point / 0x20 indexed-point /
/// 0x40 line / 0x80 polygon).
#[derive(Debug, Clone, Copy)]
pub struct Subdiv {
    /// 1-based index in file order (subdivision references are 1-based).
    pub n: u32,
    /// Byte offset of this cell's data within the RGN data section.
    pub rgn_off: u32,
    pub objtypes: u8,
    /// Cell centre longitude / latitude in mapunits.
    pub clon: i32,
    pub clat: i32,
    pub width: u16,
    pub height: u16,
    pub terminate: bool,
    pub next: u16,
    /// Index into [`Tre::levels`].
    pub level: u8,
}

/// Map bounding box in signed int24 mapunits (`deg = mu * 360 / 2^24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bbox {
    pub north: i32,
    pub east: i32,
    pub south: i32,
    pub west: i32,
}

/// The decoded TRE: bounding box, level pyramid, and every subdivision.
#[derive(Debug, Clone)]
pub struct Tre {
    pub bbox: Bbox,
    pub levels: Vec<Level>,
    pub subdivs: Vec<Subdiv>,
}

/// Parse a TRE subfile.
pub fn parse(tre: &[u8]) -> Result<Tre, GarminError> {
    if tre.len() < 0x31 {
        return Err(GarminError::Truncated("TRE header"));
    }
    let bbox = Bbox {
        north: i24le(tre, 0x15),
        east: i24le(tre, 0x18),
        south: i24le(tre, 0x1B),
        west: i24le(tre, 0x1E),
    };
    let lvl_off = u32le(tre, 0x21) as usize;
    let lvl_size = u32le(tre, 0x25) as usize;
    let sub_off = u32le(tre, 0x29) as usize;
    let sub_size = u32le(tre, 0x2D) as usize;

    // Level table: 4-byte records over [lvl_off, lvl_off + lvl_size).
    let mut levels = Vec::new();
    let lvl_end = lvl_off.saturating_add(lvl_size);
    let mut o = lvl_off;
    while o + 4 <= lvl_end && o + 4 <= tre.len() {
        levels.push(Level {
            zoom: tre[o] & 0x7F,
            inherited: tre[o] & 0x80 != 0,
            bits: tre[o + 1],
            nsubdiv: u16le(tre, o + 2),
        });
        o += 4;
    }

    // Subdivision table: 16-byte records, last (most-detailed) level 14-byte.
    let mut subdivs = Vec::new();
    let sub_end = sub_off.saturating_add(sub_size);
    let mut o = sub_off;
    let mut n = 1u32;
    let nlev = levels.len();
    for (li, level) in levels.iter().enumerate() {
        let last = li + 1 == nlev;
        let rec = if last { 14 } else { 16 };
        for _ in 0..level.nsubdiv {
            if o + rec > sub_end || o + rec > tre.len() {
                break;
            }
            let wraw = u16le(tre, o + 10);
            subdivs.push(Subdiv {
                n,
                rgn_off: u24le(tre, o) & 0x0FFF_FFFF,
                objtypes: tre[o + 3],
                clon: i24le(tre, o + 4),
                clat: i24le(tre, o + 7),
                width: wraw & 0x7FFF,
                height: u16le(tre, o + 12) & 0x7FFF,
                terminate: wraw & 0x8000 != 0,
                next: if last { 0 } else { u16le(tre, o + 14) },
                level: li as u8,
            });
            o += rec;
            n += 1;
        }
    }

    Ok(Tre {
        bbox,
        levels,
        subdivs,
    })
}
