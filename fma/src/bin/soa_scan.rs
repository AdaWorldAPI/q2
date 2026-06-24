// soa_scan.rs — 1M-row SoA scalability PoC: key-only scan vs full-value scan.
//
// Proves the OGAR canon's "the key prerenders nodes with ZERO value decode": when the
// canonical NodeRow (512 B = key 16 + edges 16 + value 480) is laid out COLUMNAR
// (struct-of-arrays) — a contiguous key column + a contiguous value column — a key-only
// scan (prefix-route / render-select, e.g. "draw the skeleton subtree" = classid 0x0A02)
// touches ~30x less memory than materializing the value slab, and stays flat as N grows.
// This is the same prefix routing the /fma-body skeleton button and `graph 00000a02` do,
// measured at scale.
//
// 1M is SYNTHETIC — real FMA is ~1368 placed meshes / ~75K terms; the scan throughput is
// the point, not data realism. Synthetic rows are seeded by the FMA addressing
// distribution (~25% skeleton classid, the rest soft tissue).
//
//   usage: soa_scan [max_n] [reps]   (default 1_000_000 rows, 5 reps; lower max_n for less RAM)

use std::hint::black_box;
use std::time::Instant;

const CLASSID_SOFT: u32 = 0x0000_0A01;
const CLASSID_SKELETON: u32 = 0x0000_0A02;

/// Columnar SoA: the 16-byte GUID key column and the 480-byte value-slab column, kept
/// in separate contiguous allocations (the "SoA > tenant view" split).
fn build(n: usize) -> (Vec<[u8; 16]>, Vec<[u8; 480]>) {
    let mut keys = Vec::with_capacity(n);
    let mut values = Vec::with_capacity(n);
    for i in 0..n {
        // synthetic canonical GUID seeded by i: ~25% skeleton, rest soft tissue.
        let classid = if i % 4 == 0 { CLASSID_SKELETON } else { CLASSID_SOFT };
        let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15); // splitmix-ish spread
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&classid.to_le_bytes()); // 0..4 classid
        k[4..12].copy_from_slice(&h.to_le_bytes()); // 4..12 HEEL/HIP/TWIG + family low
        let id = (i as u32) & 0x00FF_FFFF;
        k[12..15].copy_from_slice(&id.to_le_bytes()[..3]); // identity (u24)
        k[15] = (h >> 56) as u8;
        keys.push(k);
        // value slab: filled from i so the read can't be elided to a no-op.
        let mut v = [0u8; 480];
        v[0] = (i & 0xFF) as u8;
        v[239] = (h & 0xFF) as u8;
        v[479] = (i >> 8) as u8;
        values.push(v);
    }
    (keys, values)
}

fn gbps(bytes: usize, secs: f64) -> f64 {
    (bytes as f64) / secs / 1e9
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let max_n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let reps: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    println!("# SoA scalability: key-only (16 B/row, prefix-route) vs value (480 B/row, decode the slab)");
    println!("# NodeRow = 512 B = key(16) + edges(16) + value(480); columnar SoA. {reps} reps, best-of.");
    println!("{:>10} | {:>20} | {:>20} | {:>8}", "rows", "key-only (route)", "value (decode slab)", "speedup");
    println!("{:->10}-+-{:->20}-+-{:->20}-+-{:->8}", "", "", "", "");

    let scales: Vec<usize> = [64_000usize, 256_000, max_n].into_iter().filter(|&n| n > 0).collect();
    for &n in &scales {
        let (keys, values) = build(n);

        // key-only scan: prefix-route — count the skeleton subtree, reading only the key
        // column (classid at bytes 0..4; the 16-byte key column is what's streamed).
        let mut best_key = f64::MAX;
        let mut routed = 0usize;
        for _ in 0..reps {
            let t = Instant::now();
            let mut c = 0usize;
            for k in &keys {
                let classid = u32::from_le_bytes([k[0], k[1], k[2], k[3]]);
                if classid == CLASSID_SKELETON {
                    c += 1;
                }
            }
            black_box(c);
            best_key = best_key.min(t.elapsed().as_secs_f64());
            routed = c;
        }

        // value scan: materialize the slab — sum all 480 bytes per row (the work a value
        // decode / tenant read does), reading the whole value column.
        let mut best_val = f64::MAX;
        for _ in 0..reps {
            let t = Instant::now();
            let mut s = 0u64;
            for v in &values {
                let mut acc = 0u64;
                for &b in v.iter() {
                    acc = acc.wrapping_add(b as u64);
                }
                s = s.wrapping_add(acc);
            }
            black_box(s);
            best_val = best_val.min(t.elapsed().as_secs_f64());
        }

        let key_bytes = n * 16; // key column streamed
        let val_bytes = n * 480; // value column streamed
        let key_rps = n as f64 / best_key;
        let val_rps = n as f64 / best_val;
        println!(
            "{:>10} | {:>8.0} M/s {:>5.1} GB/s | {:>8.0} M/s {:>5.1} GB/s | {:>6.1}x",
            n,
            key_rps / 1e6,
            gbps(key_bytes, best_key),
            val_rps / 1e6,
            gbps(val_bytes, best_val),
            best_val / best_key,
        );
        let _ = routed;
    }
    println!("# key-only touches 16 B/row vs 480 B/row (30x less); routing/render-select needs NO value decode.");
}
