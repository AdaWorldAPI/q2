import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { DemoApp } from './DemoApp';
import { PalantirApp } from './PalantirApp';
import { NeuralDebuggerPage } from './NeuralDebuggerPage';
import { RenderPage, OrbitPage, FlightPage } from './RenderPage';
import { OsintScene3D } from './OsintScene3D';
import { OsintGraph } from './OsintGraph';
import { FmaGraph } from './FmaGraph';
import { TorsoMesh } from './TorsoMesh';
import { TorsoSplat } from './TorsoSplat';
import { TorsoRender } from './TorsoRender';
import { TorsoMap } from './TorsoMap';
import { FmaBody } from './FmaBody';
import { BodyV3 } from './BodyV3';
import BodyHelix from './BodyHelix';
import GenomeHelix from './GenomeHelix';
import { CpicCockpit } from './CpicCockpit';
import { ReasoningPage } from './ReasoningPage';
import { ErrorBoundary } from './components/ErrorBoundary';
import './styles/cockpit.css';
import './styles/palantir.css';
import './styles/diagnostics.css';

/** Last-resort fallback when the entire app crashes. Renders without React context. */
function RootFallback({ error, scope, reset }: { error: Error; scope: string; reset: () => void }) {
  return (
    <div style={{
      minHeight: '100vh',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      background: '#0a0e14',
      color: '#e0e6ed',
      fontFamily: 'monospace',
      padding: 24,
    }}>
      <div style={{ maxWidth: 720, border: '1px solid #ff637d44', borderRadius: 8, padding: 24, background: '#0e1219' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
          <span style={{ width: 8, height: 8, borderRadius: 4, background: '#ff637d' }} />
          <strong style={{ color: '#ff637d' }}>q2 cockpit crashed at root</strong>
          <span style={{ color: '#666', fontSize: 11, marginLeft: 'auto' }}>scope: {scope}</span>
        </div>
        <pre style={{ background: '#000', padding: 12, borderRadius: 4, fontSize: 11, color: '#ffb547', whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
{error.message}
{'\n\n'}
{error.stack?.split('\n').slice(0, 8).join('\n')}
        </pre>
        <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
          <button onClick={reset} style={{ padding: '6px 12px', background: '#1a2030', color: '#e0e6ed', border: '1px solid #2a3040', borderRadius: 4, cursor: 'pointer' }}>
            retry
          </button>
          <button onClick={() => window.location.reload()} style={{ padding: '6px 12px', background: '#1a2030', color: '#e0e6ed', border: '1px solid #2a3040', borderRadius: 4, cursor: 'pointer' }}>
            full reload
          </button>
          <a href="/demo-fallback" style={{ padding: '6px 12px', background: '#1a2030', color: '#ffb547', textDecoration: 'none', border: '1px solid #ffb54744', borderRadius: 4 }}>
            fallback mode →
          </a>
        </div>
        <div style={{ marginTop: 12, fontSize: 11, color: '#888' }}>
          Backend likely offline or returning malformed data. Check browser console + Network tab.
        </div>
      </div>
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ErrorBoundary scope="root" fallback={(err, scope, reset) => <RootFallback error={err} scope={scope} reset={reset} />}>
      <BrowserRouter>
        <Routes>
          {/* /demo = live infra demo | /demo-fallback = static stubs (outage fallback) */}
          <Route path="/demo" element={<DemoApp />} />
          <Route path="/demo-fallback" element={<DemoApp />} />
          <Route path="/reasoning" element={<ReasoningPage />} />
          <Route path="/debug" element={<NeuralDebuggerPage />} />
          <Route path="/render" element={<RenderPage />} />
          <Route path="/orbit" element={<OrbitPage />} />
          <Route path="/flight" element={<FlightPage />} />
          {/* Default landing = the SoA decoded into the sleek vis-network
              renderer (the Palantir look: ring nodes, wobbly edges, labels),
              fed by the baked /osint.soa bytes — same renderer, rerouted data.
              The 3D CAM reasoning scene lives on at /osint3d as the alternative. */}
          <Route path="/" element={<OsintGraph />} />
          <Route path="/osint" element={<OsintGraph />} />
          <Route path="/osint3d" element={<OsintScene3D />} />
          {/* FMA anatomy slice — part-of basin × leaf-limited global type (dual membership) */}
          <Route path="/fma" element={<FmaGraph />} />
          {/* FMA torso — real BodyParts3D anatomy. /torso-live = filled smooth triangle
              SURFACE (the hero: solid CAD-style anatomy); /torso = turntable; /torso-splat
              = the opaque surfel point cloud (kept for comparison). */}
          <Route path="/torso" element={<TorsoRender />} />
          <Route path="/torso-live" element={<TorsoMesh />} />
          <Route path="/torso-splat" element={<TorsoSplat />} />
          {/* FMA torso map — splat AS the GUID/value-tenant SoA: click a gaussian → its
              FMA node (O(1) switch into the node SoA) → label + partonomy ↔ graph */}
          <Route path="/torso-map" element={<TorsoMap />} />
          {/* MY full-body FMA viewer — solid triangle surface gated per (place:tissue)
              LAYER (skin/muscle/organ/skeleton/vessel/nerve buttons) + solid↔transparent.
              Additive; reads cockpit/public/fma_body.mesh; never touches /torso* (#57/#58). */}
          <Route path="/fma-body" element={<FmaBody />} />
          {/* /body — the FULL-RESOLUTION FMA body on the V3 substrate: ALL points
              (4.2 M-vert / 6.7 M-tri BodyParts3D surface, no decimation), every concept
              minted on the CLASSID_FMA_V3 (part_of:is_a) cascade. Reads the pre-baked
              cockpit/public/body.soa (BSO1 = V3 node table + SPM1 geometry). Polygons,
              not surfels — the successor to /torso-live's decimated 2k-concept torso. */}
          <Route path="/body" element={<BodyV3 />} />
          {/* /helix — EXPERIMENTAL sibling of /body. Same baked wire, but shades from the
              per-vertex helix-normal bytes (Fisher-2z geodesic codes) via a 256×256 LUT
              materialized once at load: one vertex-shader fetch/vert, no per-vertex decode,
              no rebake. Standalone (BodyHelix.tsx) so it can never break /body (#64). */}
          <Route path="/helix" element={<BodyHelix />} />
          {/* /geo — the OSM bake through the same BodyHelix decoder (osm_latest).
              Equivalent to /helix?scene=osm; a dedicated address so the map has a
              home without touching /helix's body slot or the /osm slippy map. */}
          <Route path="/geo" element={<BodyHelix />} />
          {/* /ice — the Iceland DEM bake (iceland_latest) through the same BodyHelix
              decoder; equivalent to /helix?scene=iceland, a dedicated home with
              height-profile beautification (ocean/moss/rock/ice terrain palette +
              procedural sky dome). Client-side render only — no re-bake. */}
          <Route path="/ice" element={<BodyHelix />} />
          {/* /cpic — CPIC pharmacogenomics cockpit (gene-first): {gene, diplotype, drug}
              → phenotype → recommendation, 2-hop NARS deduction over the real CPIC tables
              via POST /api/cpic/reason (the standalone cpic crate). Additive, gene-first
              alternative to the organ-first /fma-body. */}
          <Route path="/cpic" element={<CpicCockpit />} />
          {/* /genome — EXPERIMENTAL endless procedural double helix (GenomeHelix.tsx,
              standalone so it can never break /cpic). The GUID address space is billions
              of slots; CPIC fills almost none — so this is an infinite golden-angle
              scaffold (one instanced base-pair placed by a function of the step, windowed)
              with the real pharmacogenes lit as sparse loci. Wheel descends the 16-ary
              cascade (self-similar). Next: feed loci from /api/cpic/reason. */}
          <Route path="/genome" element={<GenomeHelix />} />
          {/* The Palantir JSON-graph cockpit (221 aiwar nodes) stays reachable
              at /palantir and as the catch-all for its own sub-routes. */}
          <Route path="/palantir" element={<PalantirApp />} />
          <Route path="/*" element={<PalantirApp />} />
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  </StrictMode>,
);
