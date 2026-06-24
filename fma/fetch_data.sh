#!/usr/bin/env bash
# Fetch BodyParts3D meshes and build the combined FJ->FMA map.
# Geometry (~210 MB) is NOT committed; mapping TSVs in data/ ARE.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p data/parts data/isa_parts

echo "==> is_a meshes  (~143 MB, 2234 parts)"
curl -fsSL -o /tmp/isa_BP3D.zip "https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/isa_BP3D_4.0_obj_99.zip"
unzip -q -o /tmp/isa_BP3D.zip -d data/isa_parts/

echo "==> part_of meshes  (~65 MB, 1258 parts)"
curl -fsSL -o /tmp/partof_BP3D.zip "https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/partof_BP3D_4.0_obj_99.zip"
unzip -q -o /tmp/partof_BP3D.zip -d data/parts/

echo "==> combined FJ->FMA map (part_of ∪ is_a)"
cat data/element_parts.txt > data/combined_element_parts.txt
tail -n +2 data/isa_element_parts.txt >> data/combined_element_parts.txt

echo "done. try:"
echo "  cargo run --release --bin mesh -- data/isa_parts/isa_BP3D_4.0_obj_99 data/combined_element_parts.txt data/inclusion.txt data/isa_inclusion.txt mesh bones"
echo "  cargo run --release --bin guid"
echo "  cargo run --release --bin turntable    # 270-frame 360 turntable -> fma_frames/"
echo "  PORT=8088 cargo run --release --bin serve   # /FMA /fma /fma-live"
