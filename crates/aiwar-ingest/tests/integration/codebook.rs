//! Exercises the codebook-normalized aiwar fixture (`tests/fixtures/aiwar.codebook`)
//! against the CANON mixin model: family inheritance by reference, u16 (4-nibble)
//! identity, label CAM, head-only (no serialization). Regenerate the fixture with
//! `tests/fixtures/codebook_normalize.py` from the aiwar-neo4j-harvest corpus.

use std::collections::{HashMap, HashSet};

fn fixture() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/aiwar.codebook");
    std::fs::read_to_string(path).expect("aiwar.codebook fixture present")
}

struct Node {
    identity: u16,
    family: u16,
    label: u16,
    adapters: Vec<u8>,
}

#[derive(Default)]
struct Codebook {
    families: HashMap<u16, String>,
    edge_types: HashMap<u16, String>,
    cam: HashMap<u16, String>,
    nodes: Vec<Node>,
    edges: Vec<(u16, u16)>,
}

fn hex16(s: &str) -> u16 {
    u16::from_str_radix(s.trim(), 16).expect("hex u16")
}

fn parse(text: &str) -> Codebook {
    let mut cb = Codebook::default();
    let mut section = String::new();
    for raw in text.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(s) = l.strip_prefix('@') {
            section = s.split_whitespace().next().unwrap_or("").to_string();
            continue;
        }
        match section.as_str() {
            "families" | "edge_types" => {
                let mut it = l.split_whitespace();
                let id = hex16(it.next().unwrap());
                let name = it.next().unwrap_or("").to_string();
                if section == "families" {
                    cb.families.insert(id, name);
                } else {
                    cb.edge_types.insert(id, name);
                }
            }
            "label_cam" => {
                let mut it = l.splitn(2, char::is_whitespace);
                let id = hex16(it.next().unwrap());
                cb.cam
                    .insert(id, it.next().unwrap_or("").trim().to_string());
            }
            "nodes" => {
                // 0001  f=06  l=0001  -> [06]
                let identity = hex16(l.split_whitespace().next().unwrap());
                let family = hex16(
                    l.split("f=")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap(),
                );
                let label = hex16(
                    l.split("l=")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap(),
                );
                let adapters = l
                    .split('[')
                    .nth(1)
                    .and_then(|s| s.split(']').next())
                    .map(|s| {
                        s.split(',')
                            .map(str::trim)
                            .filter(|x| !x.is_empty())
                            .map(|x| u8::from_str_radix(x, 16).expect("hex u8 adapter"))
                            .collect()
                    })
                    .unwrap_or_default();
                cb.nodes.push(Node {
                    identity,
                    family,
                    label,
                    adapters,
                });
            }
            "edges" => {
                // 006C -1-> 01EA
                let (src_part, tgt_part) = l.split_once("->").expect("edge arrow");
                let src = hex16(src_part.split('-').next().unwrap());
                let tgt = hex16(tgt_part);
                cb.edges.push((src, tgt));
            }
            _ => {}
        }
    }
    cb
}

#[test]
fn codebook_is_mixin_head_only_model() {
    let raw = fixture();
    let cb = parse(&raw);

    // seven codebook family CLASSES (the inheritable categories)
    assert_eq!(
        cb.families.len(),
        7,
        "expected 7 family classes, got {}",
        cb.families.len()
    );
    assert!(
        cb.nodes.len() > 600,
        "expected the full corpus, got {} nodes",
        cb.nodes.len()
    );
    assert!(!cb.edges.is_empty(), "edges present");
    assert!(!cb.edge_types.is_empty(), "edge-type codebook present");

    for n in &cb.nodes {
        // MIXIN: every node inherits a DEFINED family by reference, never a copy
        assert!(
            cb.families.contains_key(&n.family),
            "node {:04X} references undefined family {:02X}",
            n.identity,
            n.family
        );
        // label CAM resolves
        assert!(
            cb.cam.contains_key(&n.label),
            "node {:04X} label {:04X} not in CAM",
            n.identity,
            n.label
        );
        // out-of-family adapters resolve to DEFINED families (render-stable, family-level)
        for &a in &n.adapters {
            assert!(
                cb.families.contains_key(&u16::from(a)),
                "node {:04X} adapter {:02X} is not a family id",
                n.identity,
                a
            );
        }
    }

    // 4-nibble (u16) identities are unique discriminators
    let ids: HashSet<u16> = cb.nodes.iter().map(|n| n.identity).collect();
    assert_eq!(ids.len(), cb.nodes.len(), "u16 identities must be unique");

    // edges resolve to known identities in the CAM
    for &(s, t) in &cb.edges {
        assert!(
            cb.cam.contains_key(&s) && cb.cam.contains_key(&t),
            "edge {:04X} -> {:04X} has an endpoint missing from the CAM",
            s,
            t
        );
    }

    // HEAD-ONLY / no serialization: the fixture carries no serialized JSON properties
    assert!(
        !raw.contains("\":\"") && !raw.contains("{\""),
        "codebook must be head-only — no serialized JSON properties allowed"
    );
}
