//! cpic — shared `(part_of : is_a)` GUID toolkit for the CPIC pharmacogenomics POC.
//!
//! The canonical 16-byte `NodeGuid` (`classid·HEEL·HIP·TWIG·family·identity`, little-endian,
//! byte-identical to `lance_graph_contract::canonical_node::NodeGuid`) plus the cascade /
//! basin / mint helpers that `ingest`, `reason`, and `scan` all share.
//!
//! POC over published CPIC rules — NOT clinical decision support.

// ── classid: pharmacogenomics domain 0x0C (cf. anatomy 0x0A used by fma/converge) ──
pub const CID_GENE: u32 = 0x000C_0001;
pub const CID_ALLELE: u32 = 0x000C_0002;
pub const CID_DIPLOTYPE: u32 = 0x000C_0003;
pub const CID_PHENOTYPE: u32 = 0x000C_0004;
pub const CID_DRUG: u32 = 0x000C_0005;
pub const CID_REC: u32 = 0x000C_0006;

// ── FNV-1a (the same prefix-cascade generator the fma converge bin uses) ──
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
pub fn fnv1a(s: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// 3 cascade bytes from a slash distinguished-name: `byte[i]` = low byte of FNV-1a over the
/// cumulative prefix `seg[0]/../seg[i]`. Siblings sharing a leading prefix share leading bytes
/// → prefix-routable. Fewer than 3 segments reuse the deepest prefix.
pub fn cascade3(segs: &[String]) -> [u8; 3] {
    let mut out = [0u8; 3];
    if segs.is_empty() {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let depth = i.min(segs.len() - 1);
        *slot = (fnv1a(&segs[..=depth].join("/")) & 0xFF) as u8;
    }
    out
}

/// Family basin from a key (gene symbol, ATC root, guideline id). `family == 0` is the canon's
/// dormant default basin, so never mint it — bump a zero hash to 1.
pub fn basin(key: &str) -> u32 {
    let f = (fnv1a(key) & 0xFF_FFFF) as u32;
    if f == 0 {
        1
    } else {
        f
    }
}

/// 2^32/φ, odd ⇒ a bijective (collision-free) multiplicative mint over a per-basin counter.
pub const GOLDEN32: u32 = 0x9E37_79B9;

/// `identity(n) = n × GOLDEN32 mod 2^24` — bijective over a per-basin counter `n ∈ [1, 2^24)`.
pub fn mint_identity(counter: u32) -> u32 {
    counter.wrapping_mul(GOLDEN32) & 0xFF_FFFF
}

#[derive(Clone, Copy)]
pub struct NodeGuid {
    pub classid: u32,
    pub heel: u16,
    pub hip: u16,
    pub twig: u16,
    pub family: u32,   // u24
    pub identity: u32, // u24
}
impl NodeGuid {
    /// `part[i]` = part_of cascade byte (HIGH), `isa[i]` = is_a cascade byte (LOW) of tier i.
    pub fn mint(classid: u32, part: [u8; 3], isa: [u8; 3], family: u32, identity: u32) -> Self {
        let tile = |p: u8, i: u8| ((p as u16) << 8) | (i as u16); // (part_of : is_a)
        NodeGuid {
            classid,
            heel: tile(part[0], isa[0]),
            hip: tile(part[1], isa[1]),
            twig: tile(part[2], isa[2]),
            family: family & 0xFF_FFFF,
            identity: identity & 0xFF_FFFF,
        }
    }
    /// Canonical dotted GUID — the OGAR dash-groups (last group = family|identity).
    pub fn hex(&self) -> String {
        format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:06x}{:06x}",
            self.classid, self.heel, self.hip, self.twig, self.family, self.identity
        )
    }
    /// 16-byte little-endian key, byte-identical to `canonical_node::NodeGuid`.
    pub fn key16(&self) -> [u8; 16] {
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&self.classid.to_le_bytes());
        k[4..6].copy_from_slice(&self.heel.to_le_bytes());
        k[6..8].copy_from_slice(&self.hip.to_le_bytes());
        k[8..10].copy_from_slice(&self.twig.to_le_bytes());
        k[10..13].copy_from_slice(&self.family.to_le_bytes()[..3]);
        k[13..16].copy_from_slice(&self.identity.to_le_bytes()[..3]);
        k
    }
}

/// Letters before the first non-alphabetic char (gene-family heuristic for non-CYP/HLA genes).
pub fn alpha_prefix(s: &str) -> String {
    let p: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if p.is_empty() {
        s.to_string()
    } else {
        p
    }
}

/// The gene's `part_of` partonomy: pharmacogenome → family → … → gene. CYP genes cascade
/// family→subfamily→gene (CYP / CYP2 / CYP2C / CYP2C19) so siblings prefix-route deeply.
pub fn gene_part_of(sym: &str) -> Vec<String> {
    let mut p = vec!["pharmacogenome".to_string()];
    if sym.starts_with("CYP") {
        p.push("CYP".into());
        if sym.len() >= 4 {
            p.push(sym[..4].into());
        }
        if sym.len() >= 5 {
            p.push(sym[..5].into());
        }
        p.push(sym.into());
    } else if sym.starts_with("HLA") {
        p.push("HLA".into());
        p.push(sym.into());
    } else {
        p.push(alpha_prefix(sym));
        p.push(sym.into());
    }
    p
}

/// DN-segment normalization: lowercase, non-alnum → '_', trimmed.
pub fn norm(s: &str) -> String {
    let mapped: String = s
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    mapped.trim_matches('_').to_string()
}

/// `allele.clinicalfunctionalstatus` → the is_a functional class.
pub fn func_class(status: Option<&str>) -> String {
    let s = status.unwrap_or("").to_lowercase();
    if s.contains("no function") {
        "no_function"
    } else if s.contains("decreased") {
        "decreased_function"
    } else if s.contains("increased") {
        "increased_function"
    } else if s.contains("normal") {
        "normal_function"
    } else if s.is_empty() || s.contains("uncertain") || s.contains("unknown") {
        "uncertain_function"
    } else {
        "other_function"
    }
    .to_string()
}
