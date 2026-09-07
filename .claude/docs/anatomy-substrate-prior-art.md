# Anatomy substrate — prior art and sources

> Reference companion to `claude-notes/plans/2026-09-06-fma-bake-v4-plan.md`.
> Descriptive: what each source is, what it establishes, and which finding in
> that plan it bears on. No verdicts here — the plan owns those.

## Attribution (required, and already carried in-tree)

The body geometry is **BodyParts3D**, and its licence requires attribution
wherever the data is used. The bake tools already carry it
(`crates/osint-bake/tools/bake_torso_mesh.py:22`,
`bake_body_v3.py:25`); restated here so a reader of this doc does not have to
go find it:

> BodyParts3D, © The Database Center for Life Science, licensed under
> CC Attribution-Share Alike 2.1 Japan.

Data archive DOI: `10.18908/lsdba.nbdc00837-000`.

---

## 1. BodyParts3D — the geometry and the two relation tables

**Mitsuhashi N, Fujieda K, Tamura T, Kawamoto S, Takagi T, Okubo K.**
*BodyParts3D: 3D structure database for anatomical concepts.*
Nucleic Acids Research 2009 Jan;37(Database issue):D782-5.
doi:[10.1093/nar/gkn613](https://doi.org/10.1093/nar/gkn613) ·
PMC[2686534](https://pmc.ncbi.nlm.nih.gov/articles/PMC2686534/)

Project page: http://lifesciencedb.jp/bp3d/

Mirror used in this session: `AdaWorldAPI/BodyParts3D` (STL conversion of the
**3.0 / 20110915** release + a Julia interface). Our bakes read **4.0**
(`bake_body_v3.py:139-140` → `isa_BP3D_4.0_obj_99`, `partof_BP3D_4.0_obj_99`).

The tables under `assets/BodyParts3D_data/`, measured:

| file | lines | schema | bears on |
|---|---|---|---|
| `conventional_part_of.txt` | 2,359 | `id ⇥ name ⇥ part id ⇥ part name` | **F-3** — 2,358 edges / 1,523 nodes / **536 multi-parent** / single root `FMA20394` "human body" |
| `FMA.csv` | 104,724 | `FMAID, Preferred Label, Parent FMAID` | **F-5** — the is_a table; max FMAID 270,201 = 19 bits, which is why **F-1**'s 16-bit identity slot is a forced choice |
| `composite_parts.txt` | 12,531 | `composite id ⇥ composite name ⇥ primitive id ⇥ primitive name` | **F-4** — 8,265 of 12,530 edges add an axis word to the composite name |
| `parts_list_e.txt` | 1,524 | FMAID → English name | label slab |

Axis-word census over `composite_parts.txt` (the laterality that is *not*
consumed by any bake):

    right 3871  left 3849  proximal 311  distal 310  lateral 287  posterior 256
    anterior 250  superior 248  lower 239  upper 236  medial 196  inferior 192
    dorsal 34

The mirror's README also records, in upstream's own words, why it ships 3.0:
*"The latest version 4.0, although more complete, was not chosen as it appears
to have intersecting skin/muscle areas."* Bears on **F-7**; the plan treats a
shared cause with our 20260629b/c re-bakes as CONJECTURE pending **P-5**.

---

## 2. VOXEL-MAN / InnerOrgans — the spatial/symbolic split

**Pommert A, Höhne KH, Pflesser B, Richter E, Riemer M, Schiemann T,
Schubert R, Schumacher U, Tiede U.** *Creating a high-resolution
spatial/symbolic model of the inner organs based on the Visible Human.*
Medical Image Analysis 5(3), 2001.
PDF: https://www.virtual-body.org/media/pommert-medical-image-analysis-2001.pdf
Gallery: https://www.virtual-body.org/gallery/visible-human/torso-and-internal-organs/
Institute of Mathematics and Computer Science in Medicine (IMDM),
University Hospital Hamburg-Eppendorf.

Scale: >1000 cryosections + congruent fresh/frozen CT of the male Visible
Human; a 573×330×1049 mm³ volume; **650 anatomical constituents, >2000
relations**. (For calibration: BodyParts3D's part_of table is 1,523 nodes /
2,358 edges — same order, independent lineage.)

Three things it establishes, each with a direct counterpart in the plan:

**(a) Views as attributes of relations — bears on F-3, D-2b.** Verbatim:

> the kidneys can be seen according to structural or functional criteria:
> • regional anatomy — the kidneys are shown as part of the **abdominal viscera**
> • systemic anatomy — part of the **urogenital system**
> • relation to peritoneum — part of the **primary retroperitoneal organs**
>
> *"Views are represented as attributes of relations."*

One object under three part_of parents, the view selecting the reading. This
is the multi-parent case the cascade currently collapses, solved in 2001 by
naming the view rather than picking a winner. It is also, in one sentence, what
`ClassView` does — arrived at independently.

**(b) A separate `branching from` relation type — bears on F-3, and on the
arterial-convergence episode.** Verbatim: *"our model also contains a
'branching from' type, **modeling the arterial blood flow**."* Arterial
branching was not forced into the part hierarchy; it got its own relation.

**(c) `hidden part of` — bears on F-4.** Where a constituent is several
segmented objects with individual labels, this relation hides the technical
ones so it *"appears as one single entity."* Structurally the job
`composite_parts.txt` does upstream.

Also relevant to the vessel work (§6.4) — their segmentation is codebook +
connected-component, the same two operations in the same order as
`fill_body_soa.py`:

> *"this cluster is approximated by a **parameterized ellipsoid**, which is
> described by its center and three axis vectors."*
> *"there are other regions present in the volume which also match this
> colour-space description. If they are not connected to the target organ, it
> can be **isolated easily by a 3D connected component analysis**."*

And the scope line our bake inherits without stating: *"we decided to **model
non-segmentable objects like nerves and small blood vessels artificially** on
the basis of landmarks… their size and contrast is too small."*

Sources cited inside Pommert that matter for the symbolic half: Höhne et al.
1995 (the semantic-network knowledge base); Rosse et al. 1998 (Digital
Anatomist symbolic knowledge base); Schiemann et al. 1997 (colour-space
classification); Federative Committee on Anatomical Terminology 1998
(standardised nomenclature).

---

## 3. Gagvani & Silver — the volumetric skeleton

**Gagvani N, Silver D.** *Animating the Visible Human Dataset.*
Dept. of Electrical and Computer Engineering & CAIP Center, Rutgers.

A skeleton computed **directly from the Visible Human voxels**, imported into a
commercial animation package for skeletal animation. Their own framing of what
the artifact is:

> *"an advanced data structure for referencing all of the voxels in a
> volumetric model."*

Bears on **§6.3's explicit volumetric trie**: an address spine over a volume,
extracted rather than authored — the same role HHTL plays for us, from voxels
instead of a mesh hierarchy.

Pommert (§2) cites this work and states its limit, which is the argument for
having an address at all: per-voxel transparency *"fails to display internal
structures properly. In addition, **organ borders are not explicitly
indicated**, thus making the removal or exclusive display of an organ
impossible."* A skeleton that references every voxel still cannot answer
"show me only the liver."

---

## 4. Visible Human Project

https://www.nlm.nih.gov/research/visible/visible_human.html — NLM. The
cryosection + CT/MRI source underlying §2 and §3.

**Lineage caution.** VOXEL-MAN and Gagvani are both **voxel** models over the
Visible Human. BodyParts3D is **surface mesh**, from DBCLS/Anatomography,
independent. Our bake is on the mesh line. The convergence recorded in §2 is in
the **addressing**, not the geometry; no code path linking the lineages has
been measured, and none is claimed.
