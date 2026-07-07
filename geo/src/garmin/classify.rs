//! Typed classification: Garmin `(kind, type_code)` → a semantic [`GeoKind`]
//! with a clean palette. This is the "heuristics" stage — it replaces raster
//! colour-guessing with a **typed lookup**: a Garmin polygon *is* a building /
//! water / forest, and a polyline *is* a contour / street / stream, by its type
//! code. No inference, no colour thresholds.
//!
//! Type codes are for the OpenTopoMap / gpsfiledepot US-topo family the banked
//! `.img` sources come from (see the plan's type-code table). The classifier is
//! validated against the real village tile: the KIND histogram it produces is
//! asserted exactly in the tests.

use super::{Feature, Kind};

/// A semantic surface class for a decoded Garmin feature. The discriminant
/// order is the palette index order (see [`GeoKind::PALETTE`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoKind {
    Building,
    Water,
    Stream,
    Woods,
    Park,
    Street,
    Path,
    Contour,
    Other,
}

impl GeoKind {
    /// Canonical palette order — `GeoKind as u8` indexes this, and it is the
    /// order a bake writes into the ver-8 wire's palette block.
    pub const PALETTE: [GeoKind; 9] = [
        GeoKind::Building,
        GeoKind::Water,
        GeoKind::Stream,
        GeoKind::Woods,
        GeoKind::Park,
        GeoKind::Street,
        GeoKind::Path,
        GeoKind::Contour,
        GeoKind::Other,
    ];

    /// Palette index (`0..9`), stable across bakes.
    #[must_use]
    pub fn tag(self) -> u8 {
        self as u8
    }

    /// A sweet, clean base colour (sRGB 0..255) — the `/helix` look: no muddy
    /// browns, water reads as water, vegetation as vegetation.
    #[must_use]
    pub fn color(self) -> [u8; 3] {
        match self {
            GeoKind::Building => [200, 170, 130], // warm sandstone
            GeoKind::Water => [64, 120, 180],     // clean river blue
            GeoKind::Stream => [96, 156, 206],    // lighter running blue
            GeoKind::Woods => [72, 116, 68],      // forest green
            GeoKind::Park => [132, 176, 96],      // fresh grass green
            GeoKind::Street => [96, 96, 102],     // asphalt grey
            GeoKind::Path => [190, 150, 104],     // trail tan
            GeoKind::Contour => [168, 138, 104],  // topo brown
            GeoKind::Other => [150, 150, 150],    // neutral grey
        }
    }

    /// Whether this class fills/extrudes as an area (polygon) versus a line.
    /// Buildings extrude; water / woods / parks are flat fills; everything else
    /// is a line or point.
    #[must_use]
    pub fn is_area(self) -> bool {
        matches!(
            self,
            GeoKind::Building | GeoKind::Water | GeoKind::Woods | GeoKind::Park
        )
    }

    /// Classify a Garmin `(kind, type_code)` into a semantic class.
    #[must_use]
    pub fn classify(kind: Kind, type_code: u8) -> GeoKind {
        match kind {
            Kind::Poly => match type_code {
                0x13 | 0x60..=0x6f => GeoKind::Building, // building + urban variants
                0x3c..=0x4f => GeoKind::Water,           // sea / lake / river-fill
                0x50 => GeoKind::Woods,                  // woods / forest
                0x14..=0x1a => GeoKind::Park,            // park / reserve / grass
                _ => GeoKind::Other,
            },
            Kind::Line => match type_code {
                0x00..=0x07 => GeoKind::Street,         // roads
                0x0a | 0x0b | 0x16 => GeoKind::Path,    // trails / walking paths
                0x18 | 0x1f | 0x26 => GeoKind::Stream,  // streams / rivers
                0x20..=0x25 => GeoKind::Contour,        // land + depth contours
                _ => GeoKind::Other,
            },
            Kind::Point | Kind::IPoint => GeoKind::Other, // POIs — not a surface
        }
    }
}

impl Feature {
    /// The semantic surface class for this feature.
    #[must_use]
    pub fn geo_kind(&self) -> GeoKind {
        GeoKind::classify(self.kind, self.type_code)
    }

    /// `true` when this feature is a land/depth contour line — the elevation
    /// source for the terrain heightfield (its [`Feature::lbl`] resolves to the
    /// contour's elevation label).
    #[must_use]
    pub fn is_contour(&self) -> bool {
        self.geo_kind() == GeoKind::Contour
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_tag_round_trips() {
        for (i, k) in GeoKind::PALETTE.iter().enumerate() {
            assert_eq!(k.tag() as usize, i);
        }
    }

    #[test]
    fn spot_check_type_mappings() {
        // Representative codes from the real tile's histograms.
        assert_eq!(GeoKind::classify(Kind::Line, 0x20), GeoKind::Contour); // minor land contour
        assert_eq!(GeoKind::classify(Kind::Line, 0x22), GeoKind::Contour); // major land contour
        assert_eq!(GeoKind::classify(Kind::Line, 0x06), GeoKind::Street); // road
        assert_eq!(GeoKind::classify(Kind::Line, 0x26), GeoKind::Stream); // stream
        assert_eq!(GeoKind::classify(Kind::Line, 0x0a), GeoKind::Path); // trail
        assert_eq!(GeoKind::classify(Kind::Poly, 0x41), GeoKind::Water); // lake
        assert_eq!(GeoKind::classify(Kind::Poly, 0x14), GeoKind::Park); // park
        assert_eq!(GeoKind::classify(Kind::Poly, 0x13), GeoKind::Building); // building
        assert_eq!(GeoKind::classify(Kind::Poly, 0x50), GeoKind::Woods); // woods
        assert_eq!(GeoKind::classify(Kind::Point, 0x66), GeoKind::Other); // POI
    }

    #[test]
    fn village_tile_kind_histogram_matches_golden() {
        // The whole tile, classified — asserted against the Python golden.
        let path = format!(
            "{}/../.claude/maps/garmin-grand-canyon/47505316.img",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = super::super::Img::read(&path).expect("read tile");
        let dec = img.decode().expect("decode");

        let count = |g: GeoKind| dec.features.iter().filter(|f| f.geo_kind() == g).count();
        assert_eq!(count(GeoKind::Contour), 62_574);
        assert_eq!(count(GeoKind::Street), 32_154);
        assert_eq!(count(GeoKind::Stream), 15_161);
        assert_eq!(count(GeoKind::Path), 3_600);
        assert_eq!(count(GeoKind::Other), 3_664);
        assert_eq!(count(GeoKind::Water), 2_255);
        assert_eq!(count(GeoKind::Park), 766);
        assert_eq!(count(GeoKind::Building), 0); // wilderness tile — buildings are urban
        assert_eq!(count(GeoKind::Woods), 0);

        // Every feature classifies to exactly one KIND (partition check).
        let total: usize = GeoKind::PALETTE.iter().map(|&g| count(g)).sum();
        assert_eq!(total, dec.features.len());
        assert_eq!(total, 120_174);
    }
}
