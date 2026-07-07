//! The LBL subfile: label text. A direct port of `parse_lbl` / `label_text` in
//! `scripts/garmin_proto.py`.
//!
//! Data section at `u32@0x15`; label offsets are multiplied by `1 << b[0x1D]`;
//! encoding at 0x1E (`6` = 6-bit packed, `9` = 8-bit latin1, nul-terminated).
//! Contour labels are elevations — feet in US topo maps, metres in OTM.

use super::u32le;

/// The 6-bit label alphabet: space, A–Z, five unused, 0–9, six unused. A code
/// above `0x2F` terminates the string.
const T6: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ~~~~~0123456789~~~~~~";

/// A parsed LBL subfile: the data-section geometry plus a borrow of the bytes,
/// from which [`Lbl::text`] resolves a label offset to a string.
#[derive(Debug, Clone)]
pub struct Lbl<'a> {
    pub off: usize,
    pub len: usize,
    pub mult: usize,
    pub enc: u8,
    pub data: &'a [u8],
}

/// Parse an LBL subfile header.
#[must_use]
pub fn parse(lbl: &[u8]) -> Lbl<'_> {
    let shift = u32::from(lbl.get(0x1D).copied().unwrap_or(0));
    Lbl {
        off: u32le(lbl, 0x15) as usize,
        len: u32le(lbl, 0x19) as usize,
        mult: 1usize.checked_shl(shift).unwrap_or(1),
        enc: lbl.get(0x1E).copied().unwrap_or(0),
        data: lbl,
    }
}

impl Lbl<'_> {
    /// Resolve a label offset (as carried on a feature) to its text. Offset `0`
    /// means "no label" and yields the empty string.
    #[must_use]
    pub fn text(&self, lbloff: u32) -> String {
        if lbloff == 0 {
            return String::new();
        }
        let start = self.off + lbloff as usize * self.mult;
        let d = self.data;

        if self.enc == 9 {
            // 8-bit latin1, nul-terminated.
            match d.get(start..) {
                Some(rest) => {
                    let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
                    rest[..end].iter().map(|&c| c as char).collect()
                }
                None => String::new(),
            }
        } else {
            // 6-bit packed: 3 bytes → 4 chars, high-first.
            let mut out = String::new();
            let mut o = start;
            while o + 3 <= d.len() {
                let (b0, b1, b2) = (u32::from(d[o]), u32::from(d[o + 1]), u32::from(d[o + 2]));
                for c in [
                    b0 >> 2,
                    ((b0 & 3) << 4) | (b1 >> 4),
                    ((b1 & 0xF) << 2) | (b2 >> 6),
                    b2 & 0x3F,
                ] {
                    if c > 0x2F {
                        return out;
                    }
                    out.push(if (c as usize) < T6.len() {
                        T6[c as usize] as char
                    } else {
                        '?'
                    });
                }
                o += 3;
            }
            out
        }
    }
}
