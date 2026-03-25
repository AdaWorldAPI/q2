import { useState, useCallback } from 'react';
import { PalantirShell } from './components/PalantirShell';
import { AiwarExplorer } from './components/AiwarExplorer';
import { ReasoningNotebook } from './components/ReasoningNotebook';
import { useAiwarData } from './hooks/useAiwarData';

type View = 'shell' | 'demo' | 'aiwar' | 'notebook';

export function PalantirApp() {
  const [view, setView] = useState<View>('shell');
  const aiwar = useAiwarData();

  const handleLaunchDemo = useCallback(() => {
    window.location.href = '/demo';
  }, []);

  const handleLaunchAiwar = useCallback(async () => {
    await aiwar.load();
    setView('aiwar');
  }, [aiwar]);

  const handleLaunchNotebook = useCallback(() => {
    setView('notebook');
  }, []);

  const handleBack = useCallback(() => {
    setView('shell');
  }, []);

  if (view === 'aiwar') {
    return (
      <AiwarExplorer
        nodes={aiwar.nodes}
        edges={aiwar.edges}
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
    />
  );
}
