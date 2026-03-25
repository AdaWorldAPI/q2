import { useCallback } from 'react';
import { NeuralDebugger } from './components/debug/NeuralDebugger';

export function NeuralDebuggerPage() {
  const handleBack = useCallback(() => {
    window.location.href = '/';
  }, []);

  return <NeuralDebugger onBack={handleBack} />;
}
