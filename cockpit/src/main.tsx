import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { DemoApp } from './DemoApp';
import { PalantirApp } from './PalantirApp';
import { NeuralDebuggerPage } from './NeuralDebuggerPage';
import { RenderPage, OrbitPage, FlightPage } from './RenderPage';
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
          <Route path="/*" element={<PalantirApp />} />
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  </StrictMode>,
);
