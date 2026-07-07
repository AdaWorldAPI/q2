//! Garmin IMG map decoder — a dep-free, pure-`std` port of the validated
//! `scripts/garmin_proto.py` prototype (which renders the Grand Canyon tile
//! recognizably). Four stages, one per submodule:
//!
//! - [`container`] — the IMG container: XOR de-obfuscation, the FAT, and
//!   multi-part subfile reassembly → named subfile byte slices.
//! - [`tre`] — the TRE subfile: bounding box, the LOD level pyramid, and the
//!   subdivision tree.
//! - [`rgn`] — the RGN subfile: points / polylines / polygons, including the
//!   LSB-first delta bitstream with QMapShack `CShiftReg` continuation
//!   semantics.
//! - [`lbl`] — the LBL subfile: 6-bit-packed and 8-bit-latin1 label text.
//!
//! Garmin IMG gives the geo pipeline **typed** features — every polygon and
//! polyline carries a type code (building / water / forest / street / path /
//! contour) instead of raster colour-guessing — plus contour polylines with
//! elevation labels. Parity with the prototype is asserted byte-for-byte in the
//! tests via an FNV-1a-64 fold over every decoded coordinate.
//!
//! ```no_run
//! use geo_hhtl::garmin::{Img, Kind};
//!
//! let img = Img::read("tile.img").expect("valid IMG");
//! let decoded = img.decode().expect("TRE + RGN present");
//! let lbl = img.lbl().map(geo_hhtl::garmin::lbl::parse);
//! for f in &decoded.features {
//!     if f.kind == Kind::Line {
//!         let name = lbl.as_ref().map(|l| l.text(f.lbl)).unwrap_or_default();
//!         let _ = (f.type_code, &f.coords, name);
//!     }
//! }
//! ```

use std::collections::BTreeMap;
use std::fmt;

pub mod classify;
pub mod container;
pub mod lbl;
pub mod rgn;
pub mod tre;

pub use classify::GeoKind;
pub use tre::{Bbox, Level, Subdiv, Tre};

// ── shared little-endian byte readers ───────────────────────────────────────
// All return 0 on an out-of-bounds read; every hot-path caller guards its
// offset with an explicit length check first (mirroring the prototype's
// implicit Python slice bounds), so a 0 is only ever produced on genuinely
// malformed input, never on valid data.

pub(crate) fn u16le(b: &[u8], o: usize) -> u16 {
    match b.get(o..o + 2) {
        Some(s) => u16::from_le_bytes([s[0], s[1]]),
        None => 0,
    }
}

pub(crate) fn u32le(b: &[u8], o: usize) -> u32 {
    match b.get(o..o + 4) {
        Some(s) => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        None => 0,
    }
}

pub(crate) fn i16le(b: &[u8], o: usize) -> i16 {
    match b.get(o..o + 2) {
        Some(s) => i16::from_le_bytes([s[0], s[1]]),
        None => 0,
    }
}

pub(crate) fn u24le(b: &[u8], o: usize) -> u32 {
    match b.get(o..o + 3) {
        Some(s) => u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16,
        None => 0,
    }
}

pub(crate) fn i24le(b: &[u8], o: usize) -> i32 {
    let v = u24le(b, o) as i32;
    if v >= 1 << 23 {
        v - (1 << 24)
    } else {
        v
    }
}

/// Convert signed int24 mapunits to degrees (`deg = mu * 360 / 2^24`).
#[must_use]
pub fn mu2deg(mu: i32) -> f64 {
    f64::from(mu) * 360.0 / f64::from(1i32 << 24)
}

// ── feature model ───────────────────────────────────────────────────────────

/// The object kind of a decoded feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Point,
    IPoint,
    Line,
    Poly,
}

impl Kind {
    /// A stable one-byte tag (used in the parity fold and for compact keys).
    #[must_use]
    pub fn tag(self) -> u8 {
        match self {
            Kind::Point => 0,
            Kind::IPoint => 1,
            Kind::Line => 2,
            Kind::Poly => 3,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Point => "point",
            Kind::IPoint => "ipoint",
            Kind::Line => "line",
            Kind::Poly => "poly",
        }
    }
}

/// One decoded map feature: its kind, Garmin type code, LOD level, coordinate
/// ring in mapunits, and label offset (`0` = no label; resolve via [`lbl::Lbl`]).
#[derive(Debug, Clone)]
pub struct Feature {
    pub kind: Kind,
    pub type_code: u8,
    pub level: u8,
    /// `(lon, lat)` mapunits; a single point for [`Kind::Point`], a ring
    /// otherwise.
    pub coords: Vec<(i64, i64)>,
    pub lbl: u32,
}

/// A fully decoded IMG: the TRE tree plus every RGN feature.
#[derive(Debug, Clone)]
pub struct Decoded {
    pub tre: Tre,
    pub features: Vec<Feature>,
}

/// Error decoding a Garmin IMG.
#[derive(Debug)]
pub enum GarminError {
    /// The `DSKIMG` container signature is missing.
    NotImg,
    /// A required subfile (`TRE` / `RGN`) is absent.
    MissingSubfile(&'static str),
    /// The image ended before a required structure could be read.
    Truncated(&'static str),
    /// The backing file could not be read.
    Io(std::io::Error),
}

impl fmt::Display for GarminError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GarminError::NotImg => write!(f, "not a Garmin IMG (missing DSKIMG signature)"),
            GarminError::MissingSubfile(s) => write!(f, "missing {s} subfile"),
            GarminError::Truncated(s) => write!(f, "truncated: {s}"),
            GarminError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for GarminError {}

impl From<std::io::Error> for GarminError {
    fn from(e: std::io::Error) -> Self {
        GarminError::Io(e)
    }
}

/// A parsed IMG container: its subfiles keyed by `name.TYPE`.
#[derive(Debug, Clone)]
pub struct Img {
    pub subfiles: BTreeMap<String, Vec<u8>>,
}

impl Img {
    /// Parse an in-memory IMG image.
    pub fn parse(bytes: &[u8]) -> Result<Self, GarminError> {
        Ok(Img {
            subfiles: container::read_img(bytes)?,
        })
    }

    /// Read and parse an IMG file from disk.
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<Self, GarminError> {
        let bytes = std::fs::read(path)?;
        Self::parse(&bytes)
    }

    /// The first subfile whose name ends with `suffix` (e.g. `".TRE"`).
    #[must_use]
    pub fn subfile(&self, suffix: &str) -> Option<&[u8]> {
        self.subfiles
            .iter()
            .find(|(k, _)| k.ends_with(suffix))
            .map(|(_, v)| v.as_slice())
    }

    #[must_use]
    pub fn tre(&self) -> Option<&[u8]> {
        self.subfile(".TRE")
    }

    #[must_use]
    pub fn rgn(&self) -> Option<&[u8]> {
        self.subfile(".RGN")
    }

    #[must_use]
    pub fn lbl(&self) -> Option<&[u8]> {
        self.subfile(".LBL")
    }

    /// Decode the TRE tree and every RGN feature.
    pub fn decode(&self) -> Result<Decoded, GarminError> {
        let tre = tre::parse(self.tre().ok_or(GarminError::MissingSubfile("TRE"))?)?;
        let rgn_bytes = self.rgn().ok_or(GarminError::MissingSubfile("RGN"))?;
        let features = rgn::parse(rgn_bytes, &tre, None);
        Ok(Decoded { tre, features })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte parity fold over every decoded coordinate — identical to
    /// the Python golden generator (kind tag, type byte, then each `(lon, lat)`
    /// as little-endian `i64`).
    fn fnv1a64_coords(feats: &[Feature]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let byte = |b: u8, h: &mut u64| {
            *h ^= u64::from(b);
            *h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for f in feats {
            byte(f.kind.tag(), &mut h);
            byte(f.type_code, &mut h);
            for &(lo, la) in &f.coords {
                for b in lo.to_le_bytes() {
                    byte(b, &mut h);
                }
                for b in la.to_le_bytes() {
                    byte(b, &mut h);
                }
            }
        }
        h
    }

    fn tile(name: &str) -> Img {
        let path = format!(
            "{}/../.claude/maps/garmin-grand-canyon/{name}.img",
            env!("CARGO_MANIFEST_DIR")
        );
        Img::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn base_bits_matches_imgformat() {
        // base 0-9 → base+2; base >9 → 2*base-9.
        assert_eq!(super::rgn::testonly_base_bits(0), 2);
        assert_eq!(super::rgn::testonly_base_bits(9), 11);
        assert_eq!(super::rgn::testonly_base_bits(10), 11);
        assert_eq!(super::rgn::testonly_base_bits(15), 21);
    }

    #[test]
    fn village_tile_matches_prototype_golden() {
        let img = tile("47505316");

        // Container: exact subfile sizes.
        assert_eq!(img.rgn().map(<[u8]>::len), Some(7_705_413));
        assert_eq!(img.tre().map(<[u8]>::len), Some(28_349));
        assert_eq!(img.lbl().map(<[u8]>::len), Some(42_669));

        let dec = img.decode().expect("decode");

        // TRE: bounding box (degrees), level pyramid, subdivision count.
        assert!((mu2deg(dec.tre.bbox.north) - 36.3428).abs() < 1e-3);
        assert!((mu2deg(dec.tre.bbox.east) - -111.7941).abs() < 1e-3);
        assert!((mu2deg(dec.tre.bbox.south) - 35.3265).abs() < 1e-3);
        assert!((mu2deg(dec.tre.bbox.west) - -112.8268).abs() < 1e-3);

        let expect_levels = [
            Level {
                zoom: 4,
                inherited: true,
                bits: 17,
                nsubdiv: 1,
            },
            Level {
                zoom: 3,
                inherited: false,
                bits: 18,
                nsubdiv: 2,
            },
            Level {
                zoom: 2,
                inherited: false,
                bits: 20,
                nsubdiv: 30,
            },
            Level {
                zoom: 1,
                inherited: false,
                bits: 22,
                nsubdiv: 220,
            },
            Level {
                zoom: 0,
                inherited: false,
                bits: 23,
                nsubdiv: 763,
            },
        ];
        assert_eq!(dec.tre.levels, expect_levels);
        assert_eq!(dec.tre.subdivs.len(), 1016);

        // RGN: feature counts by kind (bitstream-sensitive).
        assert_eq!(dec.features.len(), 120_174);
        let count = |k: Kind| dec.features.iter().filter(|f| f.kind == k).count();
        assert_eq!(count(Kind::Line), 114_616);
        assert_eq!(count(Kind::Poly), 3_185);
        assert_eq!(count(Kind::Point), 2_373);

        // Per-level feature counts (level 0's lone subdiv carries no data).
        let per_level = |lv: u8| dec.features.iter().filter(|f| f.level == lv).count();
        assert_eq!(per_level(1), 371);
        assert_eq!(per_level(2), 6_461);
        assert_eq!(per_level(3), 29_829);
        assert_eq!(per_level(4), 83_513);

        // First feature, concrete.
        let f0 = &dec.features[0];
        assert_eq!(f0.kind, Kind::Line);
        assert_eq!(f0.type_code, 2);
        assert_eq!(f0.level, 1);
        assert_eq!(f0.coords.len(), 3);
        assert_eq!(f0.coords[0], (-5_223_552, 1_659_840));

        // The killer assertion: every coordinate, byte-for-byte vs the Python
        // golden. If the CShiftReg continuation/sign logic drifts at all, this
        // diverges immediately.
        assert_eq!(fnv1a64_coords(&dec.features), 0xadb3_68a3_b063_c74d);

        // LBL: the first labelled highway resolves to a real road name.
        let lbl = lbl::parse(img.lbl().unwrap());
        assert_eq!(lbl.enc, 9);
        assert_eq!(lbl.mult, 1);
        assert_eq!(lbl.text(19_437), "US Hwy 180");
    }

    #[test]
    fn other_canyon_tiles_decode_to_expected_counts() {
        for (name, rgn_len, feats) in [
            ("47505310", 9_024_634usize, 116_845usize),
            ("47505311", 3_871_943, 74_948),
            ("47505317", 4_150_505, 63_149),
        ] {
            let img = tile(name);
            assert_eq!(
                img.rgn().map(<[u8]>::len),
                Some(rgn_len),
                "tile {name} RGN size"
            );
            let dec = img
                .decode()
                .unwrap_or_else(|e| panic!("decode {name}: {e}"));
            assert_eq!(dec.features.len(), feats, "tile {name} feature count");
        }
    }
}
