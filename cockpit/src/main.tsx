import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { DemoApp } from './DemoApp';
import { PalantirApp } from './PalantirApp';
import { NeuralDebuggerPage } from './NeuralDebuggerPage';
import './styles/cockpit.css';
import './styles/palantir.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/demo" element={<DemoApp />} />
        <Route path="/debug" element={<NeuralDebuggerPage />} />
        <Route path="/*" element={<PalantirApp />} />
      </Routes>
    </BrowserRouter>
  </StrictMode>,
);
