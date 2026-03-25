import { TruthBadge } from './TruthBadge';
import type { EnrichmentEdge } from '../data/aiwar-seed';

interface ReasoningTraceProps {
  edges: EnrichmentEdge[];
  title?: string;
}

export function ReasoningTrace({ edges, title }: ReasoningTraceProps) {
  const byType = {
    deduction: edges.filter((e) => e.inference === 'deduction'),
    induction: edges.filter((e) => e.inference === 'induction'),
    abduction: edges.filter((e) => e.inference === 'abduction'),
  };

  return (
    <div className="reasoning-trace">
      {title && <div className="section-label">{title}</div>}
      {Object.entries(byType).map(([type, items]) =>
        items.length > 0 ? (
          <div key={type} className="trace-group">
            <div className="trace-group-label">
              {type === 'deduction' ? '\u26A1' : type === 'induction' ? '\uD83D\uDD0D' : '\u2753'}{' '}
              {type} ({items.length})
            </div>
            {items.map((e, i) => (
              <div key={i} className="trace-item">
                <div className="trace-edge">
                  {e.source} &rarr; [{e.label}] &rarr; {e.target}
                </div>
                <div className="trace-detail">{e.detail}</div>
                <TruthBadge f={e.truthValue.f} c={e.truthValue.c} gate={e.gate} />
              </div>
            ))}
          </div>
        ) : null,
      )}
    </div>
  );
}
