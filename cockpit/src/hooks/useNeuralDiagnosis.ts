import { useState, useCallback } from 'react';

// Types matching neural-debug's diagnosis.rs output
export interface FunctionMeta {
  id: string;
  file: string;
  line: number;
  signature: string;
  state: 'dead' | 'stub' | 'nan' | 'static' | 'alive' | 'wired_unused';
}

export interface ModuleDiagnosis {
  name: string;
  repo: string;
  total: number;
  alive_or_static: number;
  dead: number;
  stub: number;
  nan_risk: number;
  health_pct: number;
  dead_functions: FunctionMeta[];
}

export interface RepoDiagnosis {
  name: string;
  total_functions: number;
  total_dead: number;
  total_stub: number;
  total_nan_risk: number;
  health_pct: number;
  modules: ModuleDiagnosis[];
}

export interface StackDiagnosis {
  total_functions: number;
  total_files: number;
  total_dead: number;
  total_stub: number;
  total_nan_risk: number;
  health_pct: number;
  scan_duration_ms: number;
  repos: RepoDiagnosis[];
}

export function useNeuralDiagnosis() {
  const [diagnosis, setDiagnosis] = useState<StackDiagnosis | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch('/neural_diagnosis.json');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: StackDiagnosis = await res.json();
      setDiagnosis(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load');
    } finally {
      setLoading(false);
    }
  }, []);

  return { diagnosis, loading, error, load };
}
