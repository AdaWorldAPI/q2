# Torso anatomy coverage — structural gap analysis (2026-06-24)

> Prompted by comparison against high-fidelity atlases (anatomytool.org "Open
> 3D Man", VOXEL-MAN inner-organs). Goal: understand what our BodyParts3D torso
> splat is MISSING in structure. We are NOT using their geometry — pure diagnostic.

## Headline finding

**The gaps are not a source gap — they are a ROOT-SELECTION gap.** We baked the
`FMA7181` "trunk" subtree = **178 of 1368 mesh-bearing concepts (13%)**. The
"missing" organs, vessels, and nerves are NOT absent from BodyParts3D; they are
in the download we already have, hanging off **sibling system branches** we
didn't include.

BodyParts3D part-of tree = one root `FMA20394` "human body" (1368 mesh-concepts,
17,943 meshes). Its system branches:

| mesh-concepts | FMA | branch | in our bake? |
|---:|---|---|---|
| 293 | FMA7161 | cardiovascular system (heart + arterial/venous trees) | ~3 (heart only) |
| 251 | FMA231424 | body proper (trunk wall + skeleton) | **most of our 178** |
| 141 | FMA7152 | alimentary system (liver, stomach, intestine, pancreas, esophagus) | 0 |
| 103 | FMA7154 | head | 0 (out of region) |
| 99 | FMA7482 | musculoskeletal system | partial |
| 92 | FMA7158 | respiratory system (lungs, bronchi, trachea) | 0 |
| 89 | FMA7157 | nervous system (brain, cord, named nerves) | 0 |
| 66/59 | FMA7185/6 | upper limbs | 0 (out of region) |
| 51/46 | FMA7187/8 | lower limbs | 0 (out of region) |
| 47 | FMA228642 | vasculature of body | 0 |
| 10 | FMA7159 | urinary system (kidneys, bladder, ureters) | 0 |
| 6 | FMA79063 | deep fascial system | 0 |
| 4 | FMA9668 | endocrine system | 0 |

## Per-system probe (present in download / under trunk / baked)

| system | mesh-concepts in download | under FMA7181 trunk | baked |
|---|---:|---:|---:|
| arteries (aorta/artery*) | 313 | 0 | 0 |
| veins (vena cava/vein*) | 102 | 2 | 2 |
| respiratory (lung+bronchi+trachea) | 64 | 0 | 0 |
| intestine | 27 | 0 | 0 |
| lumbar vertebrae | 12 | 0 | 0 |
| liver | 10 | 0 | 0 |
| pancreas | 6 | 0 | 0 |
| kidney | 2 | 0 | 0 |
| stomach / esophagus / gallbladder / sacrum | 1 each | 0 | 0 |
| diaphragm | 1 | 1 | **1** |
| spleen | **0** | 0 | 0 |

## What's TRULY missing (vs. just un-baked)

- **Almost nothing is a real source gap.** Organs, the 293-concept arterial
  tree, the 89-concept nervous system — all present in the download, just under
  sibling roots. Reachable for free.
- **Genuine source gaps** (where even full BodyParts3D < Open 3D Man):
  - **spleen**: 0 meshes — actually absent.
  - **fidelity / density**: BodyParts3D ships 99%-decimated meshes; peripheral
    nerves are major-trunk-only (89 concepts = brain + cord + named nerves), not
    the fine plexus threads the sculpted atlases show. Fascia/ligaments thin (6).

## Why the trunk bake excluded them

`FMA7181` "trunk" is a structural/wall decomposition: skeleton + body wall +
heart-in-mediastinum. It treats the cavities' contents as undivided bags
("content of abdomen", "content of posterior mediastinum" = single mesh blobs).
The actual organ meshes are reached via the **system** roots (cardiovascular /
respiratory / alimentary / urinary / nervous), which are spatially in the trunk
but topologically siblings of FMA7181.

## Actionable conclusion

To match the Open 3D Man's layered organs + vessels + nerves we do **not** need
their (NC/ND-licensed) geometry. We **expand the bake** to include the
thoraco-abdominal members of the organ/vessel/nerve systems (spatially filter
each system's meshes to the trunk bbox, union with the current 178). All free,
CC-BY, FMA-keyed, through the existing pipeline. We are currently rendering 13%
of the anatomy we already hold.

Premium licensed geometry (VOXEL-MAN / Open 3D Man, bought commercially when the
project funds it) is only needed for the last mile: spleen, fine peripheral
nerves, and mesh fidelity — and it drops into the SAME geometry-agnostic pipeline
(OBJ + vn in), so it's a swap, not a rebuild.

## Licensing note (for the PoC → commercial path)

- **Gap analysis itself binds nothing** — studying any atlas to find gaps is not
  use/redistribution. Free to look.
- **BodyParts3D is CC-BY**: commercial-safe, survives the PoC→commercial
  transition with no relicensing landmine. This is a reason to build on it now.
- **VOXEL-MAN is CC BY-NC-ND**: ND blocks redistributing any *baked/modified*
  form even non-commercially (a shared PoC demo = redistribution of a
  derivative); NC blocks commercial. CC-ND is **not pay-to-unlock** — but UKE
  Hamburg almost certainly dual-licenses (free CC for academic, separate paid
  commercial license direct from the institution). "Pharma buys the license"
  = that separate commercial contract, which is real and standard.
- Keep the pipeline geometry-agnostic so premium meshes drop in at
  productization without touching the splat/codec/FMA-keying architecture.
