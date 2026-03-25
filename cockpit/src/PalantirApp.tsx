import { useState, useCallback } from 'react';
import { PalantirShell } from './components/PalantirShell';
import { AiwarExplorer } from './components/AiwarExplorer';
import { ReasoningNotebook } from './components/ReasoningNotebook';
import { useAiwarData } from './hooks/useAiwarData';

type View = 'shell' | 'aiwar' | 'notebook';

export function PalantirApp() {
  const [view, setView] = useState<View>('shell');
  const aiwar = useAiwarData();

  const handleLaunchDemo = useCallback(() => {
    window.location.href = '/demo';
  }, []);

  const handleLaunchAiwar = useCallback(async () => {
    const result = await aiwar.load();
    if (result) {
      setView('aiwar');
    }
  }, [aiwar]);

  const handleLaunchNotebook = useCallback(() => {
    setView('notebook');
  }, []);

  const handleBack = useCallback(() => {
    setView('shell');
  }, []);

  // Loading overlay
  if (aiwar.loading) {
    return (
      <div className="palantir-loading">
        <div className="nars-spinner" />
        <div style={{ marginTop: 16, color: 'var(--accent-2)', fontSize: 14 }}>
          Loading AIWAR dataset...
        </div>
        <div style={{ marginTop: 8, color: 'var(--muted)', fontSize: 12 }}>
          51 AI weapons systems &middot; 221 nodes &middot; 356 edges
        </div>
      </div>
    );
  }

  // Error state
  if (aiwar.error && view === 'shell') {
    // Show error but still render the shell
  }

  if (view === 'aiwar') {
    return (
      <AiwarExplorer
        nodes={aiwar.nodes}
        edges={aiwar.edges}
        weapons={aiwar.weapons}
        onBack={handleBack}
      />
    );
  }

  if (view === 'notebook') {
    return <ReasoningNotebook onBack={handleBack} />;
  }

  return (
    <PalantirShell
      onLaunchDemo={handleLaunchDemo}
      onLaunchAiwar={handleLaunchAiwar}
      onLaunchNotebook={handleLaunchNotebook}
      error={aiwar.error}
    />
  );
}
