# .claude/maps — source provenance

Git-tracked map artifacts (each file < 100 MB — GitHub blob limit). The
Dockerfile copies/reassembles what the deploy serves; the bakers read the
rest at bake time.

## Garmin IMG sources (the ground-up typed rebuild, 2026-07-07)

Garmin IMG carries **typed vector data** (every polygon/polyline has a type
code — building / water / forest / street / walking path / contour line) plus
multi-level generalization (TRE levels ≈ LOD pyramid) and, in newer builds,
per-tile `.DEM` elevation subfiles. Typed codes replace color heuristics:
one typed polygon = one building; contours → interpolated heightfield.

| file | source | contents |
|---|---|---|
| `otm-iceland.zip` | <https://garmin.opentopomap.org/europe/iceland/otm-iceland.zip> | `otm-iceland.img` (71.9 MB): 7 map tiles `534008xx`, each with TRE/RGN/LBL/NET/NOD **/DEM**, + `OPENTOPO.TYP` style. mkgmap-built, unencrypted (xor 0x00), blocksize 2048. |
| `otm-iceland-contours.zip` | <https://garmin.opentopomap.org/europe/iceland/otm-iceland-contours.zip> | `otm-iceland-contours.img` (26.8 MB): 16 contour tiles `533500xx` (TRE/RGN/LBL) + `CONTOURS.TYP`. Elevation contour polylines. |
| `garmin-grand-canyon/475053{10,11,16,17}.img` | <https://www.gpsfiledepot.com/maps/view/1> (Arizona Topo, NSIS installer, 186 MB — NOT committed; tiles extracted via `7z x`) | The 4 tiles covering the Grand Canyon. `47505316` = Grand Canyon Village (N 36.343 S 35.327 W −112.827 E −111.794); `47505317` = North Rim strip; `47505310`/`47505311` = western canyon. 2016 build: no DEM subfile — heights are **contour polylines** in RGN. |

Other free Garmin map source (not yet used): <https://alternativaslibres.org/en/downloads.php>.

## Existing artifacts (pre-Garmin arc)

| file | role |
|---|---|
| `iceland_dem_16m.z01` + `iceland_dem_16m.zip` | 2-part split zip of `iceland_dem.helix.soa.gz` (151 MB, BSO2 ver-7, 16.5M verts). Reassemble: `zip -s 0 iceland_dem_16m.zip --out full.zip && unzip full.zip`. The Dockerfile does this into `cockpit/dist/`. |
| `berlin-latest.osm.pbf` | Raw OSM PBF for the Berlin bakes (94 MB). |
| `iceland-dem-cop30-100m-isn93.tif` / `iceland-hillshade-cop30-100m-isn93.tif` | Copernicus 30 m DEM + hillshade (ISN93). |
| `iceland.helix.soa.gz` | OLD sparse Iceland scatter (unreferenced by the deploy). |
