//! cpic scan — 1M-patient cohort: key-only `(part_of:is_a)` prefix scan vs value-decode.
//!
//! Each synthetic patient is a `NodeRow` = key(16) + value(496) [the canon's 512-byte node].
//! The key is a phenotype `(part_of:is_a)` GUID (classid=PHENOTYPE, gene-family basin); the
//! value is the patient's gene_result `consultationtext` — the bulky, genuinely-skippable
//! column. The cohort query "how many of N patients are CYP2C19 Poor Metabolizer?" runs two
//! ways:
//!   key-only : 13-byte prefix-route on the key column (16 MB, cache-resident) — zero value decode.
//!   value    : decode the 496-byte consult-text slab per row (N×496 B, RAM-bound), then match.
//! This is the canon's "the key prerenders nodes with zero value decode" at cohort scale, with
//! a value that is genuinely worth skipping.
//!
//! POC over published CPIC rules — NOT clinical decision support.
//!
//! Usage:  scan [n_patients=1000000] [reps=5]

use cpic::{basin, cascade3, gene_part_of, norm, NodeGuid, CID_PHENOTYPE, GOLDEN32};
use serde_json::Value;
use std::fs;
use std::time::Instant;

const VALUE_LEN: usize = 496; // 512-byte node = key(16) + value(496)

/// SplitMix64 — deterministic patient assignment (no Date/rand dependency).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn load(path: &str) -> Vec<Value> {
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

struct Pheno {
    prefix13: [u8; 13], // classid(4) + HEEL/HIP/TWIG(6) + family(3) — the routable key, sans identity
    slab: [u8; VALUE_LEN], // "gene|result|consultationtext", padded — the value column
    label: String,
}

fn build_phenos(dir: &str) -> Vec<Pheno> {
    let mut out = Vec::new();
    for v in load(&format!("{dir}/gene_result.json")) {
        let (Some(g), Some(r)) = (v["genesymbol"].as_str(), v["result"].as_str()) else {
            continue;
        };
        let consult = v["consultationtext"].as_str().unwrap_or("");
        let mut part = gene_part_of(g);
        part.push("phenotypes".into());
        part.push(norm(r));
        let isa = vec!["phenotype".to_string(), norm(r)];
        let gd = NodeGuid::mint(CID_PHENOTYPE, cascade3(&part), cascade3(&isa), basin(g), 0);
        let k = gd.key16();
        let mut prefix13 = [0u8; 13];
        prefix13.copy_from_slice(&k[0..13]); // identity (13..16) excluded — that's per-patient
        let mut slab = [0u8; VALUE_LEN];
        let text = format!("{g}|{r}|{consult}");
        let b = text.as_bytes();
        let n = b.len().min(VALUE_LEN);
        slab[..n].copy_from_slice(&b[..n]);
        out.push(Pheno {
            prefix13,
            slab,
            label: format!("{g} {r}"),
        });
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let reps: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let dir = if std::path::Path::new("data/gene_result.json").exists() {
        "data"
    } else {
        "cpic/data"
    };

    let phenos = build_phenos(dir);
    assert!(!phenos.is_empty());

    // synthesize the cohort SoA: key column (16 B/row) + value column (496 B/row)
    let mut keys = vec![0u8; n * 16];
    let mut values = vec![0u8; n * VALUE_LEN];
    let mut rng = Rng(0xC91C_5EED);
    for i in 0..n {
        let ph = &phenos[(rng.next() as usize) % phenos.len()];
        let k = &mut keys[i * 16..i * 16 + 16];
        k[0..13].copy_from_slice(&ph.prefix13);
        let id = (i as u32).wrapping_mul(GOLDEN32) & 0xFF_FFFF; // basin-local identity
        k[13..16].copy_from_slice(&id.to_le_bytes()[..3]);
        values[i * VALUE_LEN..(i + 1) * VALUE_LEN].copy_from_slice(&ph.slab);
    }

    // cohort query: CYP2C19 Poor Metabolizer (an actionable, dose-changing phenotype)
    let target = phenos
        .iter()
        .find(|p| p.label == "CYP2C19 Poor Metabolizer")
        .unwrap_or(&phenos[0]);
    let tgt_prefix = target.prefix13;
    let (g, r) = target
        .label
        .split_once(' ')
        .unwrap_or((target.label.as_str(), ""));
    let marker = format!("{g}|{r}|");

    println!(
        "cpic scan — cohort {n} patients · query: {} · NodeRow 512 B = key 16 + value {VALUE_LEN}",
        target.label
    );
    println!(
        "  key column {:.0} MB · value column {:.0} MB",
        (n * 16) as f64 / 1e6,
        (n * VALUE_LEN) as f64 / 1e6
    );
    println!("POC over published CPIC rules — NOT clinical decision support.\n");

    // ── key-only prefix scan: 13 bytes/row, NEVER touches the value column ──
    let (mut key_count, mut key_ns) = (0usize, u128::MAX);
    for _ in 0..reps {
        let t = Instant::now();
        let mut c = 0usize;
        for i in 0..n {
            if keys[i * 16..i * 16 + 13] == tgt_prefix {
                c += 1;
            }
        }
        key_count = c;
        key_ns = key_ns.min(t.elapsed().as_nanos());
    }

    // ── value-decode scan: bring each 496 B slab into cache (the "decompress"), then match ──
    let (mut val_count, mut val_ns, mut sink) = (0usize, u128::MAX, 0u64);
    let mb = marker.as_bytes();
    for _ in 0..reps {
        let t = Instant::now();
        let (mut c, mut h) = (0usize, 0u64);
        for i in 0..n {
            let slab = &values[i * VALUE_LEN..(i + 1) * VALUE_LEN];
            for &b in slab {
                h = h.rotate_left(5) ^ b as u64; // touch ALL 496 B (models decode/decompress)
            }
            if slab.starts_with(mb) {
                c += 1;
            }
        }
        val_count = c;
        sink ^= h;
        val_ns = val_ns.min(t.elapsed().as_nanos());
    }

    let rate = |ns: u128| n as f64 / (ns as f64 / 1e9) / 1e6; // M rows/s
    let gbps = |bytes: usize, ns: u128| bytes as f64 / ns as f64; // bytes/ns = GB/s
    println!("  key-only (prefix-route)  {:>7} hits   {:>6.0} M rows/s · {:>5.1} GB/s   [touched 16 B/row]", key_count, rate(key_ns), gbps(n * 16, key_ns));
    println!("  value    (decode slab)   {:>7} hits   {:>6.0} M rows/s · {:>5.1} GB/s   [touched {VALUE_LEN} B/row]", val_count, rate(val_ns), gbps(n * VALUE_LEN, val_ns));
    println!(
        "\n  speedup  {:.0}× wall-clock   ({}× memory floor: {VALUE_LEN}/16 = the value column never read)",
        val_ns as f64 / key_ns as f64,
        VALUE_LEN / 16
    );
    println!(
        "  → the (part_of:is_a) key prerenders the actionable cohort with ZERO value decode (sink {sink:x})."
    );
    assert_eq!(key_count, val_count, "both scans must find the same cohort");
}
