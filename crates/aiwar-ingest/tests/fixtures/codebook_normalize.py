#!/usr/bin/env python3
"""Normalize the aiwar OSINT graph (aiwar_graph.json + 31 cypher enrichments)
into ONE codebook-based, non-serialized Gotham/neo4j test fixture.

CANON model (lance-graph contract::aiwar + E-FAMILY-ADAPTER + OGAR codebook):
  - classid OSINT = 0x0700  (NodeGuid::CLASSID_OSINT, >>8 == 0x07) — this is the
    bare u16 canon id, the HIGH half of the composed u32 classid since the
    2026-07-02 half-order flip (NodeGuid::CLASSID_OSINT == 0x0700_0000); this
    fixture never composes the full u32, so no functional change from the flip.
  - a node is its HEAD only: classid | family(mixin) | identity(u16) | edge-adapters
  - mixin: an entity inherits its category by REFERENCE (family-node id), never a copy
  - identity = 4 nibbles (u16)
  - label CAM: every distinct label is content-addressed (interned id <-> string)
  - no serialization: the 172K of JSON properties collapse to codebook ids
"""
import json, re, sys, glob, os

ROOT = os.environ.get("AIWAR_HARVEST", "/home/user/aiwar-neo4j-harvest")
# u16 canon id — the HIGH half of the composed u32 classid since the
# 2026-07-02 flip (NodeGuid::CLASSID_OSINT == 0x0700_0000). This script only
# ever emits the bare u16 (see @meta below), never the composed u32.
OSINT_CLASSID = 0x0700

# JSON N_<group> -> canonical family label (matches the cypher node labels)
CAT = {
    "N_Systems": "System", "N_Stakeholders": "Stakeholder", "N_People": "Person",
    "N_Civic": "CivicSystem", "N_Historical": "HistoricalSystem",
}
EDGE = {"E_connection": "connection", "E_isDevelopedBy": "isDevelopedBy",
        "E_isDeployedBy": "isDeployedBy", "E_place": "place",
        "E_people": "people", "E_hierarchical": "hierarchical"}

entities = {}   # id -> family label  (dedup across JSON + cypher)
edges = []      # (src_id, edge_type, tgt_id)
json_bytes = 0

# ---- 1. JSON graph ----
with open(f"{ROOT}/data/aiwar_graph.json") as f:
    raw = f.read(); json_bytes = len(raw); g = json.loads(raw)
for grp, fam in CAT.items():
    for n in g.get(grp, []):
        nid = n.get("id") or n.get("name")
        if nid:
            entities[nid] = fam
for grp, et in EDGE.items():
    for e in g.get(grp, []):
        s, t = e.get("source"), e.get("target")
        if s and t:
            edges.append((s, et, t))

# ---- 2. Cypher enrichments: entity nodes + schema-dimension nodes ----
node_re = re.compile(r"\(\s*\w+\s*:\s*([A-Z][A-Za-z0-9_]+)\s*\{[^}]*?\bid:\s*'([^']+)'")
axis_re = re.compile(r":SchemaAxis\s*\{[^}]*?name:\s*'([^']+)'")
val_re  = re.compile(r":SchemaValue\s*\{[^}]*?value:\s*'([^']+)'")
schema_axes, schema_vals = set(), set()
cypher_files = sorted(glob.glob(f"{ROOT}/cypher/*.cypher"))
for cf in cypher_files:
    txt = open(cf, errors="ignore").read()
    for fam, nid in node_re.findall(txt):
        entities.setdefault(nid, fam)        # JSON wins on conflict; cypher extends
    schema_axes.update(axis_re.findall(txt))
    schema_vals.update(val_re.findall(txt))
# fold the airo/vair ontology dimension in as codebook nodes
for a in sorted(schema_axes): entities.setdefault("axis:" + a, "SchemaAxis")
for v in sorted(schema_vals): entities.setdefault("val:" + v, "SchemaValue")

# ---- 3. Build the codebook ----
families = sorted({f for f in entities.values()})         # the inheritable CLASSES
fam_id = {f: i + 1 for i, f in enumerate(families)}       # 1-based u16 family ids
edge_types = sorted({et for _, et, _ in edges})
etype_id = {et: i + 1 for i, et in enumerate(edge_types)}

# label CAM: content-addressed u16 identity per distinct entity (4 nibbles)
cam = {}                                                   # id_string -> u16
for nid in sorted(entities):
    cam[nid] = len(cam) + 1                                # 0x0001 .. (dense-low)
assert len(cam) <= 0xFFFF, f"identity overflow: {len(cam)} > u16"

# per-node out-of-family adapters (target FAMILIES, not members — render-stable)
out_adapters = {nid: [] for nid in entities}
for s, et, t in edges:
    if s in entities and t in entities:
        tf = fam_id[entities[t]] & 0xFF
        if tf not in out_adapters[s]:
            out_adapters[s].append(tf)

# ---- 4. Emit the codebook fixture ----
out = []
W = out.append
W("# aiwar.codebook  —  OSINT graph normalized to the CANON codebook model")
W("# mixin family-node inheritance · 4-nibble (u16) identity · label CAM · head-only (no serialization)")
W("#")
W("# A node is its HEAD only:  classid(OSINT=0x0700) | family(mixin ref) | identity(u16) | edge-adapters")
W("# The category is INHERITED by reference (family id), never copied. No properties are serialized.")
W("")
node_bytes = len(entities) * (2 + 1 + 2)          # identity(2)+family(1)+label(2)
adapter_bytes = sum(min(len(a), 16) for a in out_adapters.values())
cb_bytes = node_bytes + adapter_bytes + len(edges) * 5
W("@meta")
W(f"  source          aiwar_graph.json + {len(cypher_files)} cypher enrichments")
W(f"  classid         0x{OSINT_CLASSID:04X}   OSINT / Palantir-Gotham")
W(f"  identity_width  16 bit (4 nibbles)")
W(f"  families        {len(families)}")
W(f"  nodes           {len(entities)}   (entities {sum(1 for f in entities.values() if not f.startswith('Schema'))} + schema {sum(1 for f in entities.values() if f.startswith('Schema'))})")
W(f"  edges           {len(edges)}")
W(f"  json_bytes      {json_bytes}")
W(f"  codebook_bytes  ~{cb_bytes}   (head-only; rho = {cb_bytes/json_bytes:.3f} of serialized JSON)")
W("")
W("@families            # the codebook CLASSES — entities inherit these by REFERENCE (mixin)")
for f in families:
    W(f"  {fam_id[f]:02X}  {f}")
W("")
W("@edge_types")
for et in edge_types:
    W(f"  {etype_id[et]:X}  {et}")
W("")
W("@label_cam           # content-addressable labels:  u16 identity  <->  name (interned, deduped)")
for nid in sorted(entities, key=lambda k: cam[k]):
    W(f"  {cam[nid]:04X}  {nid}")
W("")
W("@nodes               # identity  f=family(mixin)  l=label_cam  -> [out-of-family adapters = target families]")
for nid in sorted(entities, key=lambda k: cam[k]):
    fid = fam_id[entities[nid]]
    ad = out_adapters[nid][:16]
    adstr = "[" + ",".join(f"{b:02X}" for b in ad) + "]" if ad else "[]"
    W(f"  {cam[nid]:04X}  f={fid:02X}  l={cam[nid]:04X}  -> {adstr}")
W("")
W("@edges               # src.identity -[type]-> tgt.identity   (resolved to family adapters in the head)")
for s, et, t in edges:
    if s in cam and t in cam:
        W(f"  {cam[s]:04X} -{etype_id[et]:X}-> {cam[t]:04X}")

dst = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "aiwar.codebook")
open(dst, "w").write("\n".join(out) + "\n")
print(f"wrote {dst}")
print(f"families={len(families)} nodes={len(entities)} edges={len(edges)} "
      f"cam={len(cam)} json={json_bytes}B codebook~{cb_bytes}B rho={cb_bytes/json_bytes:.3f}")
print("families:", ", ".join(f"{f}=0x{fam_id[f]:02X}" for f in families))
