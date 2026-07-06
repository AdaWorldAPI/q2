//! The brush panel as an Askama FieldView (the OGAR
//! `CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK` pattern): ONE generated `FieldDesc`
//! table, a per-tool `u64` render mask, and a template that is a dumb loop over
//! the already-filtered field set. The mask carves which controls a tool shows
//! (Spray adds color; Ruler adds detail); the loop renders. Widget HTML per
//! [`FieldKind`] is computed in Rust ([`SculptPage::control`]) so the template
//! carries zero per-field conditionals — values are numeric/enum only, no
//! user-authored content, so the `safe` fragment has no XSS surface.

use crate::sculpt::Tool;

/// How a field is edited. The `idx` in [`FieldDesc`] is the mask bit position.
pub enum FieldKind {
    Range { min: f32, max: f32, step: f32 },
    Color,
    ToolSelect,
}

pub struct FieldDesc {
    pub idx: u8,
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
}

/// The maximal, ordered field set. `idx` == bit position == array position; the
/// ONE generated source the mask indexes and the loop renders (kept in sync by
/// construction — no hand-numbered bit lives apart from its field).
pub const FIELDS: &[FieldDesc] = &[
    FieldDesc {
        idx: 0,
        key: "tool",
        label: "tool",
        kind: FieldKind::ToolSelect,
    },
    FieldDesc {
        idx: 1,
        key: "radius",
        label: "radius",
        kind: FieldKind::Range {
            min: 0.03,
            max: 0.6,
            step: 0.01,
        },
    },
    FieldDesc {
        idx: 2,
        key: "strength",
        label: "strength",
        kind: FieldKind::Range {
            min: 0.05,
            max: 1.0,
            step: 0.05,
        },
    },
    FieldDesc {
        idx: 3,
        key: "color",
        label: "color",
        kind: FieldKind::Color,
    },
    FieldDesc {
        idx: 4,
        key: "detail",
        label: "detail",
        kind: FieldKind::Range {
            min: 4.0,
            max: 64.0,
            step: 1.0,
        },
    },
];

const B_TOOL: u64 = 1 << 0;
const B_RADIUS: u64 = 1 << 1;
const B_STRENGTH: u64 = 1 << 2;
const B_COLOR: u64 = 1 << 3;
const B_DETAIL: u64 = 1 << 4;

/// Which controls a tool exposes. Every tool shows tool+radius+strength; Spray
/// adds the color swatch, Ruler adds the detail (lattice frequency) slider.
pub fn tool_mask(t: &Tool) -> u64 {
    let base = B_TOOL | B_RADIUS | B_STRENGTH;
    match t {
        Tool::Spray => base | B_COLOR,
        Tool::Ruler => base | B_DETAIL,
        Tool::Grab | Tool::Inflate | Tool::Smooth => base,
    }
}

pub struct BrushState {
    pub tool: Tool,
    pub radius: f32,
    pub strength: f32,
    pub color: [u8; 3],
    pub detail: f32,
}

impl Default for BrushState {
    fn default() -> Self {
        BrushState {
            tool: Tool::Grab,
            radius: 0.2,
            strength: 0.6,
            color: [230, 90, 60],
            detail: 16.0,
        }
    }
}

impl BrushState {
    pub fn hex(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}",
            self.color[0], self.color[1], self.color[2]
        )
    }
    fn value_of(&self, key: &str) -> f32 {
        match key {
            "radius" => self.radius,
            "strength" => self.strength,
            "detail" => self.detail,
            _ => 0.0,
        }
    }
}

#[derive(askama::Template)]
#[template(path = "sculpt.html")]
pub struct SculptPage<'a> {
    pub fields: &'a [FieldDesc],
    pub mask: u64,
    pub brush: &'a BrushState,
    pub verts: usize,
    pub tris: usize,
    pub model_name: &'a str,
}

impl<'a> SculptPage<'a> {
    pub fn new(brush: &'a BrushState, verts: usize, tris: usize, model_name: &'a str) -> Self {
        SculptPage {
            fields: FIELDS,
            mask: tool_mask(&brush.tool),
            brush,
            verts,
            tris,
            model_name,
        }
    }

    /// The mask partition: only the SELECTED fields reach the template loop.
    pub fn selected(&self) -> impl Iterator<Item = &FieldDesc> {
        self.fields
            .iter()
            .filter(move |f| self.mask & (1 << f.idx) != 0)
    }

    /// The widget for one field — computed in Rust so the template stays a
    /// conditional-free loop. All emitted values are numeric or a fixed enum.
    pub fn control(&self, f: &FieldDesc) -> String {
        match &f.kind {
            FieldKind::ToolSelect => {
                const TOOLS: [(&str, &str); 5] = [
                    ("grab", "Grab"),
                    ("inflate", "Inflate"),
                    ("smooth", "Smooth"),
                    ("spray", "Spray"),
                    ("ruler", "Ruler"),
                ];
                let cur = tool_key(&self.brush.tool);
                let mut s = String::from("<div class=\"tools\">");
                for (k, lbl) in TOOLS {
                    let on = if k == cur { " on" } else { "" };
                    s.push_str(&format!(
                        "<button class=\"tb{on}\" onclick=\"setTool('{k}')\">{lbl}</button>"
                    ));
                }
                s.push_str("</div>");
                s
            }
            FieldKind::Color => format!(
                "<input type=\"color\" value=\"#{}\" oninput=\"setBrush('color', this.value.slice(1))\">",
                self.brush.hex()
            ),
            FieldKind::Range { min, max, step } => format!(
                "<input type=\"range\" min=\"{min}\" max=\"{max}\" step=\"{step}\" value=\"{v}\" \
                 oninput=\"setBrush('{k}', this.value); this.nextElementSibling.textContent=this.value\">\
                 <span class=\"num\">{v}</span>",
                v = self.brush.value_of(f.key),
                k = f.key,
            ),
        }
    }
}

fn tool_key(t: &Tool) -> &'static str {
    match t {
        Tool::Grab => "grab",
        Tool::Inflate => "inflate",
        Tool::Smooth => "smooth",
        Tool::Spray => "spray",
        Tool::Ruler => "ruler",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;

    #[test]
    fn mask_selects_color_only_for_spray_detail_only_for_ruler() {
        assert_eq!(tool_mask(&Tool::Grab) & B_COLOR, 0);
        assert_eq!(tool_mask(&Tool::Grab) & B_DETAIL, 0);
        assert_ne!(tool_mask(&Tool::Spray) & B_COLOR, 0);
        assert_eq!(tool_mask(&Tool::Spray) & B_DETAIL, 0);
        assert_ne!(tool_mask(&Tool::Ruler) & B_DETAIL, 0);
        assert_eq!(tool_mask(&Tool::Ruler) & B_COLOR, 0);
        // Base controls always present.
        for t in [
            Tool::Grab,
            Tool::Inflate,
            Tool::Smooth,
            Tool::Spray,
            Tool::Ruler,
        ] {
            assert_eq!(
                tool_mask(&t) & (B_TOOL | B_RADIUS | B_STRENGTH),
                B_TOOL | B_RADIUS | B_STRENGTH
            );
        }
    }

    #[test]
    fn selected_count_matches_mask() {
        let b = BrushState {
            tool: Tool::Ruler,
            ..Default::default()
        };
        let page = SculptPage::new(&b, 10, 20, "sphere");
        // Ruler: tool, radius, strength, detail = 4 (not color).
        assert_eq!(page.selected().count(), 4);
    }

    #[test]
    fn page_renders() {
        let b = BrushState::default();
        let page = SculptPage::new(&b, 42, 80, "sphere");
        let html = page.render().expect("template renders");
        assert!(html.contains("42 verts"));
        assert!(html.contains("Grab"));
        assert!(html.contains("/view.png"));
        // Grab hides the color + detail controls.
        assert!(!html.contains("type=\"color\""));
    }
}
