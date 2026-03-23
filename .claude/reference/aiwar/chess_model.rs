// aiwar-neo4j-harvest/src/chess_model.rs
//
// Chess graph schema for neo4j-rs / Neo4j ingestion.
//
// Maps Stockfish-adjacent databases into the same ontology framework
// used by AI War Cloud, enabling cross-domain resonance via ladybug-rs
// fingerprints.
//
// VSA (Vector Symbolic Architecture) markers:
//   Each position gets a bitpacked fingerprint in ladybug BindSpace.
//   Opening/endgame/tactical markers are encoded as fingerprint
//   dimensions, enabling RESONATE() across chess AND AI War domains.

use serde::{Deserialize, Serialize};

// ── Opening (ECO) ────────────────────────────────────────────────

/// A named chess opening from the ECO classification.
/// Source: lichess-org/chess-openings (TSV) + hayatbiralem/eco.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opening {
    /// ECO code, e.g. "B90"
    pub eco: String,
    /// Human name, e.g. "Sicilian, Najdorf"
    pub name: String,
    /// PGN moves, e.g. "1.e4 c5 2.Nf3 d6 ..."
    pub pgn: String,
    /// UCI move sequence, e.g. "e2e4 c7c5 g1f3 ..."
    pub uci: String,
    /// EPD / FEN of the resulting position (no move counters)
    pub epd: String,
}

/// A row from the Lichess chess-openings TSV files (a.tsv .. e.tsv)
/// Fields: eco \t name \t pgn \t uci \t epd
#[derive(Debug, Clone, Deserialize)]
pub struct OpeningTsvRow {
    pub eco: String,
    pub name: String,
    pub pgn: String,
    pub uci: String,
    pub epd: String,
}

// ── Position ─────────────────────────────────────────────────────

/// A chess position node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// FEN string (canonical identifier)
    pub fen: String,
    /// Stockfish evaluation in centipawns (if available)
    pub eval_cp: Option<i32>,
    /// Mate-in-N (positive = white mates, negative = black mates)
    pub mate_in: Option<i32>,
    /// Stockfish search depth
    pub depth: Option<u32>,
    /// Game phase: "opening", "middlegame", "endgame"
    pub phase: Option<String>,
    /// Total piece count (for endgame detection)
    pub piece_count: Option<u8>,
    /// ECO code if this position belongs to a named opening
    pub eco: Option<String>,
}

// ── Lichess Evaluation ───────────────────────────────────────────

/// A single evaluation entry from the Lichess evaluation database.
/// Format: JSONL, one position per line.
/// Source: https://database.lichess.org/#evals
#[derive(Debug, Clone, Deserialize)]
pub struct LichessEval {
    pub fen: String,
    pub evals: Vec<LichessEvalEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LichessEvalEntry {
    pub pvs: Vec<LichessPv>,
    pub knodes: Option<u64>,
    pub depth: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LichessPv {
    pub moves: String,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
}

// ── Opening Graph (eco.json) ─────────────────────────────────────

/// eco.json entry: keyed by FEN, contains ECO code + moves + name
#[derive(Debug, Clone, Deserialize)]
pub struct EcoJsonEntry {
    pub eco: Option<String>,
    pub name: Option<String>,
    pub moves: Option<String>,
    pub scid: Option<String>,
}

// ── Chess Schema Ontology ────────────────────────────────────────

/// Chess-specific taxonomy axes that parallel AIRO ontology axes.
/// These become SchemaAxis nodes in the graph, with SchemaValue children.
pub const CHESS_SCHEMA_AXES: &[(&str, &[&str])] = &[
    ("game_phase",       &["Opening", "Middlegame", "Endgame"]),
    ("position_type",    &["Open", "SemiOpen", "Closed", "SemiClosed"]),
    ("pawn_structure",   &["Symmetrical", "Isolated", "Doubled", "Passed",
                           "Chain", "Islands", "Hanging", "Backward"]),
    ("tactical_theme",   &["Pin", "Fork", "Skewer", "DiscoveredAttack",
                           "DoubleCheck", "Sacrifice", "Zugzwang", "Deflection",
                           "Decoy", "Overloading", "Interference", "XRay"]),
    ("strategic_theme",  &["KingsideAttack", "QueensideAttack", "CenterControl",
                           "MinorityAttack", "PawnStorm", "Prophylaxis",
                           "Outpost", "WeakSquare", "OpenFile", "Fianchetto"]),
    ("piece_activity",   &["Active", "Passive", "Centralized", "Trapped",
                           "Coordinated", "Uncoordinated"]),
    ("king_safety",      &["Safe", "Exposed", "CastledKingside", "CastledQueenside",
                           "OppositeСastling", "KingInCenter"]),
    ("endgame_type",     &["KPvK", "KRvK", "KQvK", "KBNvK", "KRvKB",
                           "KRvKN", "KQvKR", "RookEndgame", "PawnEndgame",
                           "BishopEndgame", "KnightEndgame", "QueenEndgame"]),
    ("eco_family",       &["A", "B", "C", "D", "E"]),
    // AIRO bridge: risk assessment on chess moves
    ("move_risk",        &["Sound", "Speculative", "Dubious", "Blunder",
                           "Brilliant", "Forced", "OnlyMove"]),
];

// ── VSA / Bitpack Constants ──────────────────────────────────────

/// BindSpace prefix assignments for chess fingerprint dimensions.
/// These correspond to the fingerprint layout in CHESS_HARVEST_PLAN.md.
pub mod bindspace {
    /// Prefixes 0x00-0x0B: Raw bitboards (12 piece types × 64 squares)
    pub const PIECE_BITBOARD_BASE: u8 = 0x00;
    /// Prefix 0x0C: Pawn structure features
    pub const PAWN_STRUCTURE: u8 = 0x0C;
    /// Prefix 0x0D: King safety features
    pub const KING_SAFETY: u8 = 0x0D;
    /// Prefix 0x0E: Piece activity features
    pub const PIECE_ACTIVITY: u8 = 0x0E;
    /// Prefix 0x0F: Tactical motifs
    pub const TACTICAL_MOTIFS: u8 = 0x0F;
    /// Prefix 0x10: Material signature
    pub const MATERIAL: u8 = 0x10;
    /// Prefixes 0x11-0x13: Strategic themes
    pub const STRATEGIC_BASE: u8 = 0x11;
    /// Prefixes 0x14-0x1F: Opening/plan context
    pub const OPENING_CONTEXT_BASE: u8 = 0x14;
    /// Prefixes 0x20-0x3F: AI War cognitive bridge
    pub const AIWAR_BRIDGE_BASE: u8 = 0x20;
    /// Prefixes 0x40-0x7F: Agent decision memory
    pub const AGENT_MEMORY_BASE: u8 = 0x40;
}
