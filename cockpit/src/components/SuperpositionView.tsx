import { useMemo } from 'react';
import type { GraphNode } from '../store';
import { computeSuperposition, THINKING_STYLES, CLUSTER_COLORS, type SuperpositionResult } from '../data/thinking-styles';

interface SuperpositionViewProps {
  nodes: GraphNode[];
}

export function SuperpositionView({ nodes }: SuperpositionViewProps) {
  const results = useMemo(() => {
    const map = computeSuperposition(nodes);
    const arr = Array.from(map.values());
    return {
      consensus: arr.filter((r) => r.category === 'consensus')
        .sort((a, b) => b.consensusScore - a.consensusScore),
      faultLines: arr.filter((r) => r.category === 'fault_line')
        .sort((a, b) => Math.abs(b.fireCount - 18) - Math.abs(a.fireCount - 18)),
      blindSpots: arr.filter((r) => r.category === 'blind_spot')
        .sort((a, b) => a.fireCount - b.fireCount),
      all: map,
    };
  }, [nodes]);

  const nodeMap = useMemo(() => {
    const m = new Map<string, GraphNode>();
    nodes.forEach((n) => m.set(n.id, n));
    return m;
  }, [nodes]);

  const getLabel = (id: string) => nodeMap.get(id)?.label || id;

  const renderBar = (result: SuperpositionResult) => {
    const pct = (result.fireCount / 36) * 100;
    return (
      <div className="super-bar-track">
        <div
          className="super-bar-fill"
          style={{
            width: `${pct}%`,
            background: result.category === 'consensus' ? '#35d07f'
              : result.category === 'fault_line' ? `linear-gradient(90deg, #00d4ff ${pct}%, #ff637d ${pct}%)`
              : '#93a9bf',
          }}
        />
      </div>
    );
  };

  const renderFiredBy = (result: SuperpositionResult) => {
    const byClusters: Record<string, number> = {};
    result.firedBy.forEach((sid) => {
      const style = THINKING_STYLES.find((s) => s.id === sid);
      if (style) byClusters[style.cluster] = (byClusters[style.cluster] || 0) + 1;
    });
    return (
      <div className="super-clusters">
        {Object.entries(byClusters).sort((a, b) => b[1] - a[1]).map(([cluster, count]) => (
          <span key={cluster} className="super-cluster-tag" style={{ color: CLUSTER_COLORS[cluster] }}>
            {cluster} ({count})
          </span>
        ))}
      </div>
    );
  };

  return (
    <div className="superposition-view">
      <div className="super-header">
        <h3>Superposition &mdash; 36 Brains on One Graph</h3>
        <span className="badge">{nodes.length} nodes analyzed</span>
      </div>

      {/* Consensus */}
      <div className="super-section">
        <div className="super-section-label super-consensus-label">
          Consensus ({results.consensus.length}) &mdash; all 36 agree
        </div>
        {results.consensus.slice(0, 8).map((r) => (
          <div key={r.nodeId} className="super-row super-row--consensus">
            <span className="super-node-name">{getLabel(r.nodeId)}</span>
            <span className="super-score">{r.fireCount}/36</span>
            {renderBar(r)}
          </div>
        ))}
      </div>

      {/* Fault lines */}
      <div className="super-section">
        <div className="super-section-label super-fault-label">
          Fault Lines ({results.faultLines.length}) &mdash; styles diverge
        </div>
        {results.faultLines.slice(0, 8).map((r) => (
          <div key={r.nodeId} className="super-row super-row--fault">
            <div className="super-row-top">
              <span className="super-node-name">{getLabel(r.nodeId)}</span>
              <span className="super-score">{r.fireCount}/36</span>
            </div>
            {renderBar(r)}
            {renderFiredBy(r)}
          </div>
        ))}
      </div>

      {/* Blind spots */}
      <div className="super-section">
        <div className="super-section-label super-blind-label">
          Blind Spots ({results.blindSpots.length}) &mdash; almost no style sees
        </div>
        {results.blindSpots.slice(0, 6).map((r) => (
          <div key={r.nodeId} className="super-row super-row--blind">
            <span className="super-node-name">{getLabel(r.nodeId)}</span>
            <span className="super-score">{r.fireCount}/36</span>
            {renderBar(r)}
            {r.firedBy.length > 0 && (
              <span className="super-blind-who">
                Only: {r.firedBy.map((sid) => THINKING_STYLES.find((s) => s.id === sid)?.name).join(', ')}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
