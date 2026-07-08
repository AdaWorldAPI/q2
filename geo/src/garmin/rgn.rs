//! The RGN subfile: points, polylines, and polygons — including the LSB-first
//! delta **bitstream**. A direct port of `decode_bitstream` / `parse_rgn` in
//! `scripts/garmin_proto.py`, following QMapShack `CShiftReg` semantics exactly
//! (the continuation marker is the part naive decoders drop, turning fine
//! levels into random-walk mush).

use super::tre::{Subdiv, Tre};
use super::{i16le, u16le, u24le, u32le, Feature, Kind};

/// LSB-first bit reader over a byte slice.
struct BitReader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.d.len() * 8 - self.pos
    }

    /// Read `n` bits LSB-first. Caller must ensure `remaining() >= n`.
    fn take(&mut self, n: usize) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let byte = self.d[self.pos >> 3];
            let bit = (byte >> (self.pos & 7)) & 1;
            v |= u32::from(bit) << i;
            self.pos += 1;
        }
        v
    }
}

/// Bit-width of a delta from its 4-bit base: `base+2` for `base <= 9`, else
/// `2*base - 9` (imgformat).
fn base_bits(base: u32) -> u32 {
    if base <= 9 {
        base + 2
    } else {
        2 * base - 9
    }
}

#[cfg(test)]
pub(crate) fn testonly_base_bits(base: u32) -> u32 {
    base_bits(base)
}

/// How one axis of a polyline is encoded, read from the two leading info bits.
struct Axis {
    /// `true` → per-delta two's complement (an extra width bit, with the
    /// continuation marker). `false` → constant-sign unsigned magnitude.
    signed: bool,
    /// Fixed sign multiplier in constant-sign mode (unused when `signed`).
    konst: i64,
}

fn axis_info(br: &mut BitReader) -> Option<Axis> {
    if br.remaining() < 1 {
        return None;
    }
    if br.take(1) != 0 {
        // Constant-sign mode: one more bit picks the shared sign.
        if br.remaining() < 1 {
            return None;
        }
        let konst = if br.take(1) != 0 { -1 } else { 1 };
        Some(Axis {
            signed: false,
            konst,
        })
    } else {
        Some(Axis {
            signed: true,
            konst: 0,
        })
    }
}

/// Decode one delta on a signed axis, resolving the continuation marker: a raw
/// value equal to the sign bit alone (`1 << (n-1)`) accumulates `2^(n-1) - 1`
/// and reads again; the closing value ends the run. Returns `None` on underrun.
fn signed_delta(br: &mut BitReader, n: usize, sign: u32) -> Option<i64> {
    let mut acc: i64 = 0;
    loop {
        if br.remaining() < n {
            return None;
        }
        let tmp = br.take(n);
        if tmp != sign {
            return Some(if tmp < sign {
                acc + i64::from(tmp)
            } else {
                (i64::from(tmp) - (i64::from(sign) << 1)) - acc
            });
        }
        acc += i64::from(tmp) - 1;
    }
}

/// Decode a polyline's delta bitstream into `(dlon, dlat)` pairs in level units.
fn decode_bitstream(data: &[u8], lon_base: u32, lat_base: u32, extra_bit: bool) -> Vec<(i64, i64)> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();

    let Some(x_axis) = axis_info(&mut br) else {
        return out;
    };
    let Some(y_axis) = axis_info(&mut br) else {
        return out;
    };

    let nx = (base_bits(lon_base) + u32::from(x_axis.signed)) as usize;
    let ny = (base_bits(lat_base) + u32::from(y_axis.signed)) as usize;
    let xsign = 1u32 << (nx - 1);
    let ysign = 1u32 << (ny - 1);

    loop {
        if extra_bit {
            if br.remaining() < 1 {
                break;
            }
            br.take(1); // routing-node flag, discarded
        }

        let dlon = if x_axis.signed {
            match signed_delta(&mut br, nx, xsign) {
                Some(v) => v,
                None => break,
            }
        } else {
            if br.remaining() < nx {
                break;
            }
            i64::from(br.take(nx)) * x_axis.konst
        };

        let dlat = if y_axis.signed {
            match signed_delta(&mut br, ny, ysign) {
                Some(v) => v,
                None => break,
            }
        } else {
            if br.remaining() < ny {
                break;
            }
            i64::from(br.take(ny)) * y_axis.konst
        };

        out.push((dlon, dlat));
    }
    out
}

/// Decode every feature in an RGN subfile, driven by the TRE subdivision tree.
///
/// `want_levels`, when `Some`, restricts decoding to those LOD levels; `None`
/// decodes all of them.
pub fn parse(rgn: &[u8], tre: &Tre, want_levels: Option<&[u8]>) -> Vec<Feature> {
    let data_off = u32le(rgn, 0x15) as usize;
    let data_len = u32le(rgn, 0x19) as usize;

    // Subdivisions that own RGN data, in ascending rgn_off order; each one's
    // data ends where the next live one begins (or at the section end).
    let mut live: Vec<Subdiv> = tre
        .subdivs
        .iter()
        .filter(|s| s.objtypes != 0)
        .copied()
        .collect();
    live.sort_by_key(|s| s.rgn_off);

    let mut feats = Vec::new();
    for i in 0..live.len() {
        let s = live[i];
        if let Some(wl) = want_levels {
            if !wl.contains(&s.level) {
                continue;
            }
        }
        let rgn_end = if i + 1 < live.len() {
            live[i + 1].rgn_off as usize
        } else {
            data_len
        };
        let bits = tre.levels.get(s.level as usize).map_or(24, |l| l.bits);
        let shift = 24u32.saturating_sub(u32::from(bits));

        let block_start = data_off + s.rgn_off as usize;
        let block_end = (data_off + rgn_end).min(rgn.len());
        if block_start >= block_end {
            continue;
        }
        let block = &rgn[block_start..block_end];

        // Object kinds present, in canonical order; if K kinds, (K-1) u16
        // pointers prefix the block and split it into per-kind spans.
        let mut kinds: Vec<Kind> = Vec::new();
        for (kind, mask) in [
            (Kind::Point, 0x10),
            (Kind::IPoint, 0x20),
            (Kind::Line, 0x40),
            (Kind::Poly, 0x80),
        ] {
            if s.objtypes & mask != 0 {
                kinds.push(kind);
            }
        }
        if kinds.is_empty() {
            continue;
        }
        let nptr = kinds.len() - 1;
        let ptrs: Vec<usize> = (0..nptr).map(|j| u16le(block, 2 * j) as usize).collect();

        let mut starts = Vec::with_capacity(kinds.len());
        starts.push(2 * nptr);
        starts.extend(ptrs.iter().copied());
        let mut ends = ptrs;
        ends.push(block.len());

        for (ki, &kind) in kinds.iter().enumerate() {
            let s0 = starts[ki].min(block.len());
            let e0 = ends[ki].min(block.len());
            if s0 >= e0 {
                continue;
            }
            decode_segment(&block[s0..e0], kind, &s, shift, &mut feats);
        }
    }
    feats
}

/// Decode one per-kind span of a subdivision block.
fn decode_segment(seg: &[u8], kind: Kind, s: &Subdiv, shift: u32, feats: &mut Vec<Feature>) {
    let mut o = 0usize;
    match kind {
        // Indexed points decode identically to points and are reported as points
        // (matching the prototype's `'kind': 'point'` for both).
        Kind::Point | Kind::IPoint => {
            while o + 9 <= seg.len() {
                let t = seg[o];
                let lbl = u24le(seg, o + 1);
                let has_sub = lbl & 0x80_0000 != 0;
                let dlon = i64::from(i16le(seg, o + 4));
                let dlat = i64::from(i16le(seg, o + 6));
                o += if has_sub { 9 } else { 8 };
                let lon = i64::from(s.clon) + (dlon << shift);
                let lat = i64::from(s.clat) + (dlat << shift);
                feats.push(Feature {
                    kind: Kind::Point,
                    type_code: t,
                    level: s.level,
                    coords: vec![(lon, lat)],
                    lbl: lbl & 0x3F_FFFF,
                });
            }
        }
        Kind::Line | Kind::Poly => {
            let type_mask: u8 = if kind == Kind::Line { 0x3F } else { 0x7F };
            while o + 10 <= seg.len() {
                let b0 = seg[o];
                let t = b0 & type_mask;
                let two = b0 & 0x80 != 0;
                let lblraw = u24le(seg, o + 1);
                let extra = kind == Kind::Line && lblraw & 0x40_0000 != 0;
                let lbl = lblraw & 0x3F_FFFF;
                let dlon = i64::from(i16le(seg, o + 4));
                let dlat = i64::from(i16le(seg, o + 6));

                // Bitstream byte length: two-byte when b0 bit7 is set. The info
                // byte (lon/lat base nibbles) is separate from the length count.
                let (blen, o2) = if two {
                    (u16le(seg, o + 8) as usize, o + 10)
                } else {
                    (seg[o + 8] as usize, o + 9)
                };
                let info = seg.get(o2).copied().unwrap_or(0);
                let lon_base = u32::from(info & 0x0F);
                let lat_base = u32::from(info >> 4);
                let bs_start = o2 + 1;
                let bs_end = bs_start.saturating_add(blen).min(seg.len());
                let bs: &[u8] = if bs_start <= seg.len() {
                    &seg[bs_start..bs_end]
                } else {
                    &[]
                };
                o = o2 + 1 + blen;

                let mut lon = i64::from(s.clon) + (dlon << shift);
                let mut lat = i64::from(s.clat) + (dlat << shift);
                let mut coords = vec![(lon, lat)];
                for (dlo, dla) in decode_bitstream(bs, lon_base, lat_base, extra) {
                    lon += dlo << shift;
                    lat += dla << shift;
                    coords.push((lon, lat));
                }
                feats.push(Feature {
                    kind,
                    type_code: t,
                    level: s.level,
                    coords,
                    lbl,
                });
            }
        }
    }
}
