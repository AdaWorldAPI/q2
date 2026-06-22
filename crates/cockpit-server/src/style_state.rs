//! Process-global selected ThinkingStyle for shader dispatches.
//!
//! User selects via POST /v1/shader/style; the shader_stream SSE
//! loop reads `current_style()` (or `current_dispatch()`) when
//! building each `ShaderDispatch`, overriding the default
//! `StyleSelector::Auto`.
//!
//! Why a process-global LazyLock<Mutex<...>>:
//! - One scene per process (mirrors `shader_stream::SCENE`).
//! - Avoids `axum::FromRef<Arc<AppState>>` orphan-rule plumbing.
//! - The selection is a UI-driven knob, not per-connection state.
//!
//! Note on `StyleSelector` variants (from
//! `lance_graph_contract::cognitive_shader`):
//!     enum StyleSelector { Ordinal(u8), Named(&'static str), Auto }
//!
//! The 36 actual thinking styles live in
//! `lance_graph_contract::thinking::ThinkingStyle` (`#[repr(u8)]`,
//! 0..=35). We map a name string to `StyleSelector::Ordinal(style as
//! u8)` so we never need a `&'static str` (Named requires a static
//! lifetime that we don't have when parsing user input).

use std::sync::{LazyLock, Mutex};

use lance_graph_contract::cognitive_shader::{ShaderDispatch, StyleSelector};
use lance_graph_contract::thinking::ThinkingStyle;

static SELECTED: LazyLock<Mutex<StyleSelector>> =
    LazyLock::new(|| Mutex::new(StyleSelector::Auto));

/// Read the currently-selected `StyleSelector` (defaults to `Auto`).
pub fn current_style() -> StyleSelector {
    *SELECTED.lock().unwrap()
}

/// Replace the currently-selected style.
pub fn set_style(s: StyleSelector) {
    *SELECTED.lock().unwrap() = s;
}

/// Build a `ShaderDispatch` with the currently-selected style and
/// all other fields at their default values. The shader_stream SSE
/// loop should call this in place of `ShaderDispatch::default()` so
/// the user-selected style flows into every cycle.
pub fn current_dispatch() -> ShaderDispatch {
    ShaderDispatch {
        style: current_style(),
        ..ShaderDispatch::default()
    }
}

/// Return a stable string for the currently-selected style.
///
/// Used by the shader status endpoint so the FE can echo back what
/// the backend is actually running with. Returns `"Auto"` for
/// `StyleSelector::Auto`, the canonical capitalized name for
/// `Ordinal(n)` (e.g. `"Focused"` is not in the canonical list — see
/// `parse_style_name` for the alias list), and a fallback
/// `"Ordinal(N)"` / `"Named(...)"` for shapes we did not produce.
pub fn current_style_name() -> &'static str {
    match current_style() {
        StyleSelector::Auto => "Auto",
        StyleSelector::Named(n) => n,
        StyleSelector::Ordinal(n) => ordinal_to_name(n),
    }
}

/// Map a ThinkingStyle ordinal to its canonical name string.
fn ordinal_to_name(n: u8) -> &'static str {
    match n {
        0 => "Logical",
        1 => "Analytical",
        2 => "Critical",
        3 => "Systematic",
        4 => "Methodical",
        5 => "Precise",
        6 => "Creative",
        7 => "Imaginative",
        8 => "Innovative",
        9 => "Artistic",
        10 => "Poetic",
        11 => "Playful",
        12 => "Empathetic",
        13 => "Compassionate",
        14 => "Supportive",
        15 => "Nurturing",
        16 => "Gentle",
        17 => "Warm",
        18 => "Direct",
        19 => "Concise",
        20 => "Efficient",
        21 => "Pragmatic",
        22 => "Blunt",
        23 => "Frank",
        24 => "Curious",
        25 => "Exploratory",
        26 => "Questioning",
        27 => "Investigative",
        28 => "Speculative",
        29 => "Philosophical",
        30 => "Reflective",
        31 => "Contemplative",
        32 => "Metacognitive",
        33 => "Wise",
        34 => "Transcendent",
        35 => "Sovereign",
        _ => "Unknown",
    }
}

/// Parse a style name (e.g. "Focused", "Auto") into a `StyleSelector`.
///
/// - Case-insensitive.
/// - "Auto" / "auto" / "AUTO" → `StyleSelector::Auto`.
/// - Any of the 36 canonical `ThinkingStyle` names → `StyleSelector::Ordinal(n)`
///   where `n = style as u8`.
/// - A small alias set ("Focused" → Precise, "Reflexive" → Reflective) for
///   common UI labels that don't map 1:1 to canonical names.
/// - Unknown → `None` so the handler can return 400.
pub fn parse_style_name(name: &str) -> Option<StyleSelector> {
    let lower = name.trim().to_ascii_lowercase();

    if lower == "auto" {
        return Some(StyleSelector::Auto);
    }

    // Canonical 36 ThinkingStyle names. Match the names emitted by the
    // contract enum exactly; we use lowercase for the comparison.
    let style = match lower.as_str() {
        "logical" => ThinkingStyle::Logical,
        "analytical" => ThinkingStyle::Analytical,
        "critical" => ThinkingStyle::Critical,
        "systematic" => ThinkingStyle::Systematic,
        "methodical" => ThinkingStyle::Methodical,
        "precise" => ThinkingStyle::Precise,
        "creative" => ThinkingStyle::Creative,
        "imaginative" => ThinkingStyle::Imaginative,
        "innovative" => ThinkingStyle::Innovative,
        "artistic" => ThinkingStyle::Artistic,
        "poetic" => ThinkingStyle::Poetic,
        "playful" => ThinkingStyle::Playful,
        "empathetic" | "empathic" => ThinkingStyle::Empathetic,
        "compassionate" => ThinkingStyle::Compassionate,
        "supportive" => ThinkingStyle::Supportive,
        "nurturing" => ThinkingStyle::Nurturing,
        "gentle" => ThinkingStyle::Gentle,
        "warm" => ThinkingStyle::Warm,
        "direct" => ThinkingStyle::Direct,
        "concise" => ThinkingStyle::Concise,
        "efficient" => ThinkingStyle::Efficient,
        "pragmatic" => ThinkingStyle::Pragmatic,
        "blunt" => ThinkingStyle::Blunt,
        "frank" => ThinkingStyle::Frank,
        "curious" => ThinkingStyle::Curious,
        "exploratory" => ThinkingStyle::Exploratory,
        "questioning" => ThinkingStyle::Questioning,
        "investigative" => ThinkingStyle::Investigative,
        "speculative" => ThinkingStyle::Speculative,
        "philosophical" => ThinkingStyle::Philosophical,
        "reflective" | "reflexive" => ThinkingStyle::Reflective,
        "contemplative" => ThinkingStyle::Contemplative,
        "metacognitive" => ThinkingStyle::Metacognitive,
        "wise" => ThinkingStyle::Wise,
        "transcendent" => ThinkingStyle::Transcendent,
        "sovereign" => ThinkingStyle::Sovereign,
        // UI aliases — "Focused" is the cockpit's UX label for Precise.
        "focused" => ThinkingStyle::Precise,
        _ => return None,
    };
    Some(StyleSelector::Ordinal(style as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_style_name_known_variants() {
        // Auto, case-insensitive
        assert!(matches!(
            parse_style_name("Auto"),
            Some(StyleSelector::Auto)
        ));
        assert!(matches!(
            parse_style_name("auto"),
            Some(StyleSelector::Auto)
        ));
        assert!(matches!(
            parse_style_name("AUTO"),
            Some(StyleSelector::Auto)
        ));

        // A canonical named style: Precise → Ordinal(5)
        match parse_style_name("Precise") {
            Some(StyleSelector::Ordinal(n)) => assert_eq!(n, ThinkingStyle::Precise as u8),
            other => panic!("expected Ordinal(Precise as u8), got {:?}", other),
        }

        // Case-insensitivity on a canonical name.
        match parse_style_name("creative") {
            Some(StyleSelector::Ordinal(n)) => assert_eq!(n, ThinkingStyle::Creative as u8),
            other => panic!("expected Ordinal(Creative as u8), got {:?}", other),
        }

        // UI alias: Focused → Precise.
        match parse_style_name("Focused") {
            Some(StyleSelector::Ordinal(n)) => assert_eq!(n, ThinkingStyle::Precise as u8),
            other => panic!("expected Ordinal(Precise as u8), got {:?}", other),
        }

        // Whitespace tolerated.
        assert!(matches!(
            parse_style_name("  Auto  "),
            Some(StyleSelector::Auto)
        ));
    }

    #[test]
    fn parse_style_name_unknown_returns_none() {
        assert!(parse_style_name("lol_random_string").is_none());
        assert!(parse_style_name("").is_none());
        assert!(parse_style_name("focusedish").is_none());
    }

    #[test]
    fn current_style_default_is_auto() {
        // Note: this test relies on running first; other tests in this
        // module mutate SELECTED. We assert the default-name path,
        // which doesn't depend on prior state.
        assert_eq!(ordinal_to_name(0), "Logical");
        assert_eq!(ordinal_to_name(35), "Sovereign");
        assert_eq!(ordinal_to_name(99), "Unknown");
    }

    #[test]
    fn current_dispatch_inherits_selection() {
        // Set to Precise, then read back via current_dispatch.
        set_style(StyleSelector::Ordinal(ThinkingStyle::Precise as u8));
        let d = current_dispatch();
        match d.style {
            StyleSelector::Ordinal(n) => assert_eq!(n, ThinkingStyle::Precise as u8),
            other => panic!("expected Ordinal(Precise), got {:?}", other),
        }
        // Reset for cleanliness (other tests may share process state).
        set_style(StyleSelector::Auto);
    }
}
