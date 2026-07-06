#!/usr/bin/env python3
"""One-shot rebake: store OSINT-v3 GUID tier bytes in DECLARED field order (hi/lo fix).

Context
-------
`data/osint-v3/` is baked from the external `AdaWorldAPI/aiwar-neo4j-harvest`
graph (no in-repo baker; regeneration is not possible offline). The original
bake wrote each `6×(8:8)` tier as a **little-endian u16** — i.e. the two bytes
of every `hi:lo` pair landed in memory as `(lo, hi)`. But the contract's
position law (`lance-graph-contract` `ClassView::facet_rows`: field position i =
facet byte i) and the field cards declared in
`crates/cockpit-server/src/osint_classview.rs` (`OSINT_SYSTEM_FIELDS`,
`OSINT_PERSON_FIELDS`) read position i as facet byte i in `hi:lo` order. The two
disagree by a byte swap within each tier pair.

Decoding the ORIGINAL bake against the codebook's declared order lands only
16/611 GUID1 and 52/133 GUID2 values in-vocabulary; swapping each tier pair's
two bytes raises that to 609/611 and 133/133. The operator ruled: fix the DATA
(this rebake), not the field declarations.

What this does
--------------
For every record in `osint_v3.soa` AND every node in `osint_v3_nodes.json`:
  * bytes 0..4 of each 16-byte GUID (the classid u32, LE 0x07011000) are LEFT
    UNTOUCHED;
  * the 12 tier bytes 4..16 are swapped pairwise within each of the 6 tier
    pairs: (4,5) (6,7) (8,9) (10,11) (12,13) (14,15).
GUID2 gets the identical treatment (all-zero for non-persons → swap is a no-op;
`null` in nodes.json stays `null`). The `.soa` and the nodes.json hex strings
are transformed consistently and cross-checked byte-for-byte afterwards.

Idempotency / safety
--------------------
A pairwise swap is its own inverse — running it twice would REVERT the fix. The
script therefore refuses to run unless the data is currently in the BROKEN
state (GUID1 no-swap decode rate strictly worse than swap decode rate). This is
a deliberate one-shot; the guard prevents an accidental double-application.

Usage
-----
    python3 scripts/osint_v3_rebake_hilo.py            # rebake in place
    python3 scripts/osint_v3_rebake_hilo.py --check    # report decode rates, do not write
"""
from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = REPO_ROOT / "data" / "osint-v3"
SOA = DATA_DIR / "osint_v3.soa"
NODES = DATA_DIR / "osint_v3_nodes.json"
CODEBOOK = DATA_DIR / "osint_v3_codebook.json"

HEADER_MAGIC = b"OSINTV3\0"
CLASSID_LE = 0x07011000  # canonical CLASSID_OSINT_V3
CLASSID_HEX = struct.pack("<I", CLASSID_LE).hex()  # "00100107" (LE bytes of classid)

# Tier byte-pair offsets within a 16-byte GUID (classid = bytes 0..4).
PAIR_STARTS = (4, 6, 8, 10, 12, 14)


def swap_tier_pairs(guid: bytes) -> bytes:
    """Swap the two bytes within each of the 6 tier pairs; classid (0..4) intact."""
    assert len(guid) == 16, len(guid)
    b = bytearray(guid)
    for s in PAIR_STARTS:
        b[s], b[s + 1] = b[s + 1], b[s]
    return bytes(b)


def load_codebook():
    cb = json.loads(CODEBOOK.read_text())
    airo = cb["airo_vocab"]
    mcc = cb["mcclelland_vocab"]
    g1_fields = [
        "currentStatus", "type", "militaryUse", "civicUse", "MLTask", "MLType",
        "purpose", "capacity", "output", "impact", "stakeholder", "airo_type",
    ]
    g1_max = [max(airo[f].values()) for f in g1_fields]
    g2_fields = ["stage", "need", "receptor", "rubicon", "motive"]
    g2_len = [len(mcc[f]) for f in g2_fields]
    return g1_max, g2_len


def guid1_in_vocab(tier: list[int], g1_max: list[int]) -> bool:
    # position i == field i; 0 = absent (always valid), else must be <= vocab max.
    return all(tier[i] <= g1_max[i] for i in range(12))


def guid2_in_vocab(tier: list[int], g2_len: list[int]) -> bool:
    # 1-based indexing (matches airo_vocab): value v -> vocab[v-1]; 0 = absent.
    # positions 0..5 = stage,need,receptor,rubicon,motive,padding("_").
    for i in range(5):
        v = tier[i]
        if v != 0 and v > g2_len[i]:
            return False
    return tier[5] == 0  # TWIG "_" padding must be empty in correct order


def tier_of(guid_hex: str) -> list[int]:
    return list(bytes.fromhex(guid_hex)[4:16])


def decode_rates(nodes, g1_max, g2_len):
    """(guid1_ok, guid1_ok_if_swapped, guid2_ok, guid2_ok_if_swapped, n, n_person)."""
    g1 = g1s = g2 = g2s = 0
    persons = [x for x in nodes if x.get("is_person") and x.get("guid2")]
    for x in nodes:
        t = tier_of(x["guid1"])
        ts = list(swap_tier_pairs(bytes.fromhex(x["guid1"]))[4:16])
        g1 += guid1_in_vocab(t, g1_max)
        g1s += guid1_in_vocab(ts, g1_max)
    for x in persons:
        t = tier_of(x["guid2"])
        ts = list(swap_tier_pairs(bytes.fromhex(x["guid2"]))[4:16])
        g2 += guid2_in_vocab(t, g2_len)
        g2s += guid2_in_vocab(ts, g2_len)
    return g1, g1s, g2, g2s, len(nodes), len(persons)


def cross_verify_soa_vs_nodes(raw, nodes, count, stride):
    """Byte-for-byte cross-check: every `.soa` row's stored GUID bytes must match
    the row-matched `nodes.json` hex fields.

    Rows are paired positionally (soa row i <-> nodes[i]) — the same pairing the
    transform path uses — and the row_id stored in the soa row is additionally
    required to equal ``nodes[i]["row"]``. For each row this compares GUID1 and
    GUID2 against ``guid1``/``guid2`` (``null`` guid2 must be all-zero in the
    binary) and confirms the untouched classid prefix stayed canonical.

    Returns a list of ``(row_index, row_id, field, soa_value, json_value)``
    mismatch tuples; an empty list means the binary and the JSON agree exactly.
    ``raw`` may be ``bytes`` or ``bytearray`` (the transform path passes the
    mutated ``bytearray`` before writing; --check passes the on-disk bytes).
    """
    mismatches: list[tuple] = []
    n = min(len(nodes), count)
    for i in range(n):
        x = nodes[i]
        base = 16 + i * stride
        rid = struct.unpack_from("<I", raw, base)[0]
        g1b = bytes(raw[base + 4:base + 20])
        g2b = bytes(raw[base + 20:base + 36])
        if rid != x["row"]:
            mismatches.append((i, x["row"], "row_id", rid, x["row"]))
        if g1b.hex() != x["guid1"]:
            mismatches.append((i, x["row"], "guid1", g1b.hex(), x["guid1"]))
        elif g1b[:4].hex() != CLASSID_HEX:
            mismatches.append((i, x["row"], "guid1.classid", g1b[:4].hex(), CLASSID_HEX))
        j2 = x.get("guid2")
        if j2 is None:
            if g2b != b"\0" * 16:
                mismatches.append((i, x["row"], "guid2", g2b.hex(), "null (all-zero)"))
        elif g2b.hex() != j2:
            mismatches.append((i, x["row"], "guid2", g2b.hex(), j2))
        elif g2b[:4].hex() != CLASSID_HEX:
            mismatches.append((i, x["row"], "guid2.classid", g2b[:4].hex(), CLASSID_HEX))
    return mismatches


def read_soa():
    raw = SOA.read_bytes()
    if raw[:8] != HEADER_MAGIC:
        sys.exit(f"bad magic: {raw[:8]!r}")
    ver, count, stride = struct.unpack_from("<HIH", raw, 8)
    if (ver, stride) != (3, 36):
        sys.exit(f"unexpected ver/stride: ver={ver} stride={stride} (want 3/36)")
    if len(raw) != 16 + count * stride:
        sys.exit(f"bad length {len(raw)} != {16 + count * stride}")
    return raw, count, stride


def main() -> None:
    check_only = "--check" in sys.argv[1:]
    g1_max, g2_len = load_codebook()
    raw, count, stride = read_soa()
    nodes = json.loads(NODES.read_text())
    if len(nodes) != count:
        sys.exit(f"count mismatch: soa {count} vs nodes {len(nodes)}")

    g1, g1s, g2, g2s, n, npers = decode_rates(nodes, g1_max, g2_len)
    print(f"BEFORE  nodes={n} persons={npers}")
    print(f"  GUID1 in-vocab: as-stored {g1}/{n}   if-swapped {g1s}/{n}")
    print(f"  GUID2 in-vocab: as-stored {g2}/{npers}   if-swapped {g2s}/{npers}")

    if check_only:
        # Cross-verify the .soa GUID bytes against nodes.json for EVERY row.
        # (Decode rates above only inspect nodes.json; without this a --check
        # could pass while the binary and the JSON silently disagree.)
        mism = cross_verify_soa_vs_nodes(raw, nodes, count, stride)
        if mism:
            print(f"CROSS-VERIFY FAILED: {len(mism)} mismatch(es) between "
                  f"{SOA.relative_to(REPO_ROOT)} and {NODES.relative_to(REPO_ROOT)}")
            for (i, row, field, soa_v, json_v) in mism[:5]:
                print(f"  row_index={i} row_id={row} field={field} "
                      f"soa={soa_v} json={json_v}")
            sys.exit(1)
        print(f"CROSS-VERIFY OK: {count} rows — .soa GUID1/GUID2 bytes match "
              f"nodes.json for every row")
        return

    # Guard: only transform data that is currently BROKEN (swap strictly helps).
    if not (g1s > g1 and g2s > g2):
        sys.exit(
            "REFUSING: data is not in the broken (pre-swap) state — "
            "swapping would not strictly improve decode rate. Already rebaked?"
        )

    # --- transform the .soa in place (row_id + classid intact; swap tier pairs) ---
    out = bytearray(raw)
    for i in range(count):
        g1_off = 16 + i * stride + 4       # after row_id(4)
        g2_off = g1_off + 16
        out[g1_off:g1_off + 16] = swap_tier_pairs(bytes(out[g1_off:g1_off + 16]))
        out[g2_off:g2_off + 16] = swap_tier_pairs(bytes(out[g2_off:g2_off + 16]))

    # --- transform the nodes.json hex strings consistently ---
    for x in nodes:
        x["guid1"] = swap_tier_pairs(bytes.fromhex(x["guid1"])).hex()
        if x.get("guid2") is not None:
            x["guid2"] = swap_tier_pairs(bytes.fromhex(x["guid2"])).hex()

    # --- cross-verify BEFORE writing: .soa == nodes.json for every row ---
    # Same helper --check uses, so the two paths can never drift apart.
    mism = cross_verify_soa_vs_nodes(out, nodes, count, stride)
    assert not mism, f"post-transform cross-verify failed ({len(mism)}): {mism[:5]}"

    # --- re-check decode rate on the transformed nodes ---
    g1, g1s, g2, g2s, n, npers = decode_rates(nodes, g1_max, g2_len)
    print(f"AFTER   nodes={n} persons={npers}")
    print(f"  GUID1 in-vocab: as-stored {g1}/{n}   (if-swapped {g1s}/{n})")
    print(f"  GUID2 in-vocab: as-stored {g2}/{npers}   (if-swapped {g2s}/{npers})")
    assert g1 >= 609 and g2 == npers, "post-transform decode rate below expectation"

    SOA.write_bytes(bytes(out))
    # nodes.json is written with the exact serialization of the original bake
    # (indent=1, ASCII payload, no trailing newline) so the diff is only the
    # swapped hex, never a whole-file reformat.
    NODES.write_text(json.dumps(nodes, indent=1, ensure_ascii=False))
    print(f"WROTE {SOA.relative_to(REPO_ROOT)} ({len(out)} bytes) and "
          f"{NODES.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
