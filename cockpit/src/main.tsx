import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { DemoApp } from './DemoApp';
import { PalantirApp } from './PalantirApp';
import { NeuralDebuggerPage } from './NeuralDebuggerPage';
import { RenderPage, OrbitPage, FlightPage } from './RenderPage';
import { ReasoningPage } from './ReasoningPage';
import './styles/cockpit.css';
import './styles/palantir.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
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
  </StrictMode>,
);
