#!/usr/bin/env python3
"""Extract the FMA *heart* part-of subtree from the real fma.owl into the
line-oriented Turtle subset the q2 light bake hydrates (osint-bake/src/fma_ttl.rs).

Source (provenance): fma.owl, FMA v5.0.0, BioPortal #29 / UW Structural
Informatics Group, CC BY 3.0, SHA-256 59465eb503c418dbf9216d362b94c7f42bc3b96b297e553ec17c7fee77979e44,
mirrored at github.com/AdaWorldAPI/q2/releases/download/fma-ontology-5.0.0/fma.owl

This is a one-off data-prep tool. The PRODUCTION hydrator is
lance_graph_ontology::hydrate_fma (rio_xml) at the spine; this script exists so
the light bake (no lance/datafusion closure) can carry a *real* heart subtree
instead of a hand-authored fixture. Streaming ET.iterparse keeps memory bounded
(only label / part / superclass maps are retained, not the 1.5M-triple graph).

Usage: python3 extract_fma_heart.py <fma.owl> <out.ttl>
"""
import sys
import xml.etree.ElementTree as ET

RDF = "{http://www.w3.org/1999/02/22-rdf-syntax-ns#}"
RDFS = "{http://www.w3.org/2000/01/rdf-schema#}"
OWL = "{http://www.w3.org/2002/07/owl#}"
BASE = "http://purl.org/sig/ont/fma/"

# forward "has-part" properties (whole → part); the part is a CHILD of the whole.
PART_PROPS = {"regional_part", "constitutional_part", "systemic_part"}
HEART = "fma7088"          # FMA:Heart
MAX_DEPTH = 4              # organ → chamber → wall → tissue
MAX_NODES = 80            # keep the cockpit slice legible
MAX_CHILDREN = 3          # per NON-root parent — a balanced cross-section, not a
                          # coronary-artery dump (FMA's coronary tree has ~40 named
                          # branches; uncapped BFS lets it starve every other part).
                          # The root (Heart) is exempt: all its major subdivisions
                          # — 4 chamber cavities, 4 valves, 3 septa, both coronary
                          # arteries, wall, fibrous skeleton, … — are the point.

CLEAR_TAGS = {OWL + t for t in ("Class", "Axiom", "ObjectProperty",
                                "AnnotationProperty", "DatatypeProperty",
                                "NamedIndividual")} | {RDF + "Description"}


def local(iri):
    return iri.rsplit("/", 1)[-1] if iri else None


def main(src, out):
    label, children, supers = {}, {}, {}
    for _ev, elem in ET.iterparse(src, events=("end",)):
        if elem.tag == OWL + "Class":
            about = elem.get(RDF + "about")
            if about and about.startswith(BASE):
                cid = local(about)
                lab = elem.find(RDFS + "label")
                if lab is not None and lab.text:
                    label[cid] = lab.text.strip()
                for sc in elem.findall(RDFS + "subClassOf"):
                    res = sc.get(RDF + "resource")
                    if res and res.startswith(BASE):                 # named superclass → is-a
                        supers.setdefault(cid, []).append(local(res))
                        continue
                    restr = sc.find(OWL + "Restriction")
                    if restr is None:
                        continue
                    op = restr.find(OWL + "onProperty")
                    sv = restr.find(OWL + "someValuesFrom")
                    if op is None or sv is None:
                        continue
                    prop = local(op.get(RDF + "resource") or "")
                    tgt = sv.get(RDF + "resource")
                    if prop in PART_PROPS and tgt and tgt.startswith(BASE):
                        children.setdefault(cid, []).append(local(tgt))  # tgt is part of cid
        if elem.tag in CLEAR_TAGS:
            elem.clear()

    # BFS the heart subtree; first whole to reach a part becomes its tree-parent
    # (FMA part-of is a DAG; we take a spanning tree rooted at the heart). Each
    # non-root whole contributes at most MAX_CHILDREN parts, so breadth stays
    # balanced across the heart's subdivisions instead of draining into one
    # deep subtree. Because every level-1 node is popped before any level-2 node
    # (plain BFS), this caps fan-out per parent fairly across the whole level.
    parent, depth, order = {}, {HEART: 0}, [HEART]
    queue = [HEART]
    while queue and len(order) < MAX_NODES:
        n = queue.pop(0)
        if depth[n] >= MAX_DEPTH:
            continue
        cap = MAX_NODES if n == HEART else MAX_CHILDREN   # root keeps every major part
        taken = 0
        for c in sorted(set(children.get(n, []))):
            if taken >= cap:
                break
            if c in depth or c not in label:        # unseen + actually a labelled class
                continue
            parent[c], depth[c] = n, depth[n] + 1
            order.append(c)
            queue.append(c)
            taken += 1
            if len(order) >= MAX_NODES:
                break

    def tok(cid):
        # real FMA label → a prefix:token the line-Turtle reader splits on
        # (label_of() turns '_' back into spaces). alnum kept, else '_'.
        lab = label.get(cid, cid)
        t = "".join(ch if ch.isalnum() else "_" for ch in lab).strip("_")
        while "__" in t:
            t = t.replace("__", "_")
        return "fma:" + t

    lines = [
        "# fma-heart.ttl — REAL FMA heart subtree, extracted from fma.owl (FMA v5.0.0).",
        "# Source: BioPortal #29 / UW Structural Informatics Group, CC BY 3.0,",
        "# SHA-256 59465eb503c418dbf9216d362b94c7f42bc3b96b297e553ec17c7fee77979e44,",
        "# mirror github.com/AdaWorldAPI/q2/releases/download/fma-ontology-5.0.0/fma.owl",
        "# Generated by tools/extract_fma_heart.py (streaming ET; regional_part +",
        "# constitutional_part partonomy, BFS from FMA:Heart fma7088, depth<=%d, <=%d nodes)."
        % (MAX_DEPTH, MAX_NODES),
        "# The 266 MB OWL is NOT committed; the production hydrator is the spine's",
        "# lance_graph_ontology::hydrate_fma. This is the real-data heart slice the",
        "# q2 /fma render surface hydrates.",
        "",
    ]
    seen_types = set()
    for c in order:
        if c in parent:
            lines.append(f"{tok(c)} bfo:part_of {tok(parent[c])} .")
    for c in order:                                  # cross-cutting type: first named superclass
        for s in supers.get(c, []):
            if s in label and s not in depth:        # a real category outside the part-of tree
                lines.append(f"{tok(c)} rdfs:subClassOf {tok(s)} .")
                seen_types.add(s)
                break
    open(out, "w").write("\n".join(lines) + "\n")

    # inspection summary to stderr
    print(f"heart subtree: {len(order)} nodes (<= depth {MAX_DEPTH}), "
          f"{sum(1 for c in order if c in parent)} part-of edges, "
          f"{len(seen_types)} cross-cutting types", file=sys.stderr)
    for c in order[:14]:
        print(f"  depth {depth[c]}  {label.get(c,c)}"
              + (f"  ⊂ {label.get(parent[c])}" if c in parent else "  (root)"), file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
