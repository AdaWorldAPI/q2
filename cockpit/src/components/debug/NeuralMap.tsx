import { useState, useMemo } from 'react';
import type { StackDiagnosis, RepoDiagnosis, ModuleDiagnosis } from '../../hooks/useNeuralDiagnosis';
import { NeuronGrid } from './NeuronGrid';
import { CoverageBar } from './CoverageBar';

interface NeuralMapProps {
  diagnosis: StackDiagnosis;
  onSelectModule: (repo: string, module: string) => void;
  selectedModule: string | null;
}

const REPO_LABELS: Record<string, string> = {
  'lance-graph': 'THE FACE',
  'ndarray': 'THE BODY',
  'q2': 'THE SHELL',
  'aiwar-neo4j-harvest': 'THE DATA',
};

function healthColor(pct: number): string {
  if (pct >= 70) return '#35d07f';
  if (pct >= 30) return '#ffb547';
  return '#ff637d';
}

function healthEmoji(pct: number): string {
  if (pct >= 70) return 'healthy';
  if (pct >= 30) return 'partial';
  return 'critical';
}

export function NeuralMap({ diagnosis, onSelectModule, selectedModule }: NeuralMapProps) {
  return (
    <div className="neural-map">
      {/* Stack summary */}
      <div className="neural-summary">
        <div className="neural-summary-stat">
          <span className="neural-stat-value">{diagnosis.total_functions.toLocaleString()}</span>
          <span className="neural-stat-label">functions</span>
        </div>
        <div className="neural-summary-stat">
          <span className="neural-stat-value" style={{ color: '#35d07f' }}>
            {(diagnosis.total_functions - diagnosis.total_dead - diagnosis.total_stub).toLocaleString()}
          </span>
          <span className="neural-stat-label">alive/static</span>
        </div>
        <div className="neural-summary-stat">
          <span className="neural-stat-value" style={{ color: '#ff637d' }}>{diagnosis.total_dead}</span>
          <span className="neural-stat-label">dead</span>
        </div>
        <div className="neural-summary-stat">
          <span className="neural-stat-value" style={{ color: '#93a9bf' }}>{diagnosis.total_stub}</span>
          <span className="neural-stat-label">stub</span>
        </div>
        <div className="neural-summary-stat">
          <span className="neural-stat-value" style={{ color: '#ffb547' }}>{diagnosis.total_nan_risk}</span>
          <span className="neural-stat-label">NaN risk</span>
        </div>
        <div className="neural-summary-stat">
          <span className="neural-stat-value" style={{ color: healthColor(diagnosis.health_pct) }}>
            {diagnosis.health_pct.toFixed(1)}%
          </span>
          <span className="neural-stat-label">health</span>
        </div>
      </div>

      {/* Repo cards */}
      <div className="neural-repos">
        {diagnosis.repos.map((repo) => (
          <div key={repo.name} className="neural-repo-card">
            <div className="neural-repo-header">
              <div>
                <h3 style={{ color: healthColor(repo.health_pct) }}>{repo.name}</h3>
                <span className="neural-repo-role">{REPO_LABELS[repo.name] || ''}</span>
              </div>
              <span className="neural-repo-health" style={{ color: healthColor(repo.health_pct) }}>
                {repo.health_pct.toFixed(0)}%
              </span>
            </div>
            <div className="neural-repo-stats">
              <span>{repo.total_functions} fns</span>
              <span style={{ color: '#ff637d' }}>{repo.total_dead} dead</span>
              <span style={{ color: '#93a9bf' }}>{repo.total_stub} stub</span>
              <span style={{ color: '#ffb547' }}>{repo.total_nan_risk} NaN</span>
            </div>
            <CoverageBar
              alive={repo.total_functions - repo.total_dead - repo.total_stub}
              dead={repo.total_dead}
              stub={repo.total_stub}
              nan={repo.total_nan_risk}
              total={repo.total_functions}
            />
            <div className="neural-modules">
              {repo.modules
                .filter((m) => m.total > 0)
                .sort((a, b) => a.health_pct - b.health_pct)
                .slice(0, 12)
                .map((mod) => (
                  <button
                    key={mod.name}
                    className={`neural-module-chip ${selectedModule === `${repo.name}::${mod.name}` ? 'active' : ''}`}
                    style={{ borderColor: `${healthColor(mod.health_pct)}40` }}
                    onClick={() => onSelectModule(repo.name, mod.name)}
                  >
                    <span className="neural-module-dot" style={{ background: healthColor(mod.health_pct) }} />
                    <span>{mod.name}</span>
                    <span className="neural-module-count">{mod.total}</span>
                    {mod.dead > 0 && <span className="neural-module-dead">{mod.dead}d</span>}
                    {mod.nan_risk > 0 && <span className="neural-module-nan">{mod.nan_risk}n</span>}
                  </button>
                ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
