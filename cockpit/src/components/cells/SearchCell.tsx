import { useState } from 'react';
import { TruthBadge } from '../TruthBadge';

interface ExtractedEntity {
  name: string;
  type: string;
  truth: { f: number; c: number };
}

interface ExtractedEdge {
  source: string;
  target: string;
  label: string;
  truth: { f: number; c: number };
}

interface SearchResult {
  entities: ExtractedEntity[];
  edges: ExtractedEdge[];
  sources: string[];
  llmResponse: string;
}

interface SearchCellProps {
  onExecute: (query: string, model: string) => void;
  result: SearchResult | null;
  onAddToGraph: () => void;
}

export function SearchCell({ onExecute, result, onAddToGraph }: SearchCellProps) {
  const [query, setQuery] = useState('Venezuela oil China energy access 2026');
  const [model, setModel] = useState('grok-4');
  const [loading, setLoading] = useState(false);

  const handleSearch = () => {
    setLoading(true);
    onExecute(query, model);
    setTimeout(() => setLoading(false), 2000);
  };

  return (
    <div className="nb-cell nb-cell--search">
      <div className="nb-cell-header">
        <span className="nb-cell-type nb-cell-type--search">SEARCH</span>
        <span className="nb-cell-desc">LLM + web search for HOLD items</span>
      </div>
      <div className="nb-cell-body">
        <div className="nb-cell-row">
          <input
            className="nb-input"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search query..."
          />
          <select value={model} onChange={(e) => setModel(e.target.value)} className="nb-select">
            <option value="grok-4">Grok-4 (xAI)</option>
            <option value="claude">Claude (Anthropic)</option>
          </select>
          <button className="nb-run-btn" onClick={handleSearch} disabled={loading}>
            {loading ? 'Searching...' : '\uD83D\uDD0D Search'}
          </button>
        </div>
        {result && (
          <div className="nb-cell-result">
            <div className="nb-search-response">
              <strong>Extracted:</strong>
              {result.entities.map((e, i) => (
                <div key={i} className="nb-extracted-item">
                  Node: <strong>{e.name}</strong> ({e.type})
                  <TruthBadge f={e.truth.f} c={e.truth.c} gate="HOLD" compact />
                </div>
              ))}
              {result.edges.map((e, i) => (
                <div key={i} className="nb-extracted-item">
                  Edge: {e.source} &rarr;[{e.label}]&rarr; {e.target}
                  <TruthBadge f={e.truth.f} c={e.truth.c} gate="HOLD" compact />
                </div>
              ))}
              <div className="nb-search-sources">
                Sources: {result.sources.join(', ')}
              </div>
            </div>
            <div className="nb-cell-actions">
              <button className="nb-action-btn" onClick={onAddToGraph}>Add to graph</button>
              <button className="nb-action-btn">Verify sources</button>
              <button className="nb-action-btn nb-action-btn--danger">Discard</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
