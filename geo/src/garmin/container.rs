//! The IMG container: XOR de-obfuscation, the FAT, and multi-part subfile
//! reassembly. A direct port of `read_img` in `scripts/garmin_proto.py`.
//!
//! Layout facts (hard-earned, see the plan): byte 0 = XOR key (0x00 on our
//! files); `DSKIMG` at 0x10; `blocksize = 1 << (b[0x61] + b[0x62])`; the FAT is
//! 512-byte entries from 0x600 (flag 0x01, 8-byte name + 3-byte type, size u32
//! at 0x0C **valid in the part-0 entry**, part u16 at 0x10, 240 u16 block
//! pointers at 0x20). A subfile split across parts concatenates its blocks in
//! `part*240 + i` order.

use std::collections::BTreeMap;

use super::{u16le, u32le, GarminError};

/// One accumulating FAT entry: the declared size (from the part-0 record) plus
/// the block-index → physical-block map, keyed so a `BTreeMap` iterates them in
/// `part*240 + i` order.
struct Entry {
    size: u32,
    blocks: BTreeMap<u32, u16>,
}

/// Parse the container and return `name.TYPE → bytes` for every subfile.
///
/// De-obfuscates in place when the XOR key is non-zero, verifies the `DSKIMG`
/// signature, then walks the FAT reassembling each (possibly multi-part)
/// subfile from its block list.
pub fn read_img(raw: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, GarminError> {
    if raw.len() < 0x600 {
        return Err(GarminError::Truncated("container header"));
    }

    // Byte 0 is the XOR obfuscation key; de-obfuscate the whole image if set.
    let xor = raw[0];
    let owned: Vec<u8>;
    let b: &[u8] = if xor != 0 {
        owned = raw.iter().map(|x| x ^ xor).collect();
        &owned
    } else {
        raw
    };

    if b.get(0x10..0x16) != Some(&b"DSKIMG"[..]) {
        return Err(GarminError::NotImg);
    }

    let blocksize: usize = 1usize << (u32::from(b[0x61]) + u32::from(b[0x62]));
    if blocksize == 0 {
        return Err(GarminError::Truncated("zero blocksize"));
    }

    // FAT: 512-byte entries from 0x600. flag 0x00 ends the table; only flag
    // 0x01 entries are live; the header pseudo-entry (blank / space-led name)
    // is skipped.
    let mut subs: BTreeMap<(String, String), Entry> = BTreeMap::new();
    let mut o = 0x600usize;
    while o + 512 <= b.len() {
        let flag = b[o];
        if flag != 0x01 {
            if flag == 0x00 {
                break;
            }
            o += 512;
            continue;
        }
        let first = b[o + 1];
        let name = ascii_field(&b[o + 1..o + 9]);
        let typ = ascii_field(&b[o + 9..o + 12]);
        if name.is_empty() || first == b' ' || first == 0 {
            o += 512;
            continue;
        }

        let size = u32le(b, o + 12);
        let part = u32::from(u16le(b, o + 16));
        let ent = subs.entry((name, typ)).or_insert(Entry {
            size: 0,
            blocks: BTreeMap::new(),
        });
        // The declared size lives in the part-0 record; keep it when non-zero.
        if part == 0 && size != 0 {
            ent.size = size;
        }
        // 240 block pointers per FAT entry; part N owns block indices [N*240..).
        for i in 0..240u32 {
            let blk = u16le(b, o + 0x20 + (i as usize) * 2);
            if blk != 0xFFFF {
                ent.blocks.insert(part * 240 + i, blk);
            }
        }
        o += 512;
    }

    // Reassemble each subfile by concatenating its blocks in index order, then
    // truncating to the declared size (falling back to the block-span length).
    let mut out = BTreeMap::new();
    for ((name, typ), ent) in subs {
        let mut data = Vec::new();
        // BTreeMap::values() iterates in key order (part*240 + i), i.e. the
        // correct block concatenation order.
        for &blk in ent.blocks.values() {
            let start = blk as usize * blocksize;
            let end = (start + blocksize).min(b.len());
            if start < end {
                data.extend_from_slice(&b[start..end]);
            }
        }
        let size = if ent.size != 0 {
            ent.size as usize
        } else {
            data.len()
        };
        data.truncate(size);
        out.insert(format!("{name}.{typ}"), data);
    }
    Ok(out)
}

/// Decode a fixed-width ASCII name/type field (space-padded) and trim it, the
/// Rust equivalent of `b[...].decode('ascii','replace').strip()`.
fn ascii_field(b: &[u8]) -> String {
    let text: String = b
        .iter()
        .map(|&c| if c.is_ascii() { c as char } else { '\u{FFFD}' })
        .collect();
    text.trim_matches(|c: char| c.is_ascii_whitespace())
        .to_string()
}
