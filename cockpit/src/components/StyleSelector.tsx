import { THINKING_STYLES, CLUSTER_COLORS, type ThinkingStyle } from '../data/thinking-styles';

interface StyleSelectorProps {
  selectedId: string | null;
  onSelect: (style: ThinkingStyle | null) => void;
  superpositionActive: boolean;
  onToggleSuperposition: () => void;
}

const CLUSTERS = ['Analytical', 'Empathic', 'Creative', 'Strategic', 'Critical', 'Meta'] as const;

export function StyleSelector({
  selectedId,
  onSelect,
  superpositionActive,
  onToggleSuperposition,
}: StyleSelectorProps) {
  return (
    <div className="style-selector">
      <div className="style-selector-header">
        <div className="section-label">cognitive lens</div>
        <button
          className={`style-super-btn ${superpositionActive ? 'active' : ''}`}
          onClick={onToggleSuperposition}
        >
          {superpositionActive ? 'Exit Superposition' : 'All 36 Brains'}
        </button>
      </div>

      {!superpositionActive && (
        <div className="style-clusters">
          {CLUSTERS.map((cluster) => (
            <div key={cluster} className="style-cluster">
              <div className="style-cluster-label" style={{ color: CLUSTER_COLORS[cluster] }}>
                {cluster}
              </div>
              <div className="style-cluster-items">
                {THINKING_STYLES.filter((s) => s.cluster === cluster).map((style) => (
                  <button
                    key={style.id}
                    className={`style-chip ${selectedId === style.id ? 'active' : ''}`}
                    style={{
                      borderColor: selectedId === style.id ? style.color : undefined,
                      background: selectedId === style.id ? `${style.color}18` : undefined,
                    }}
                    onClick={() => onSelect(selectedId === style.id ? null : style)}
                    title={style.description}
                  >
                    <span className="style-chip-dot" style={{ background: style.color }} />
                    {style.name}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {selectedId && !superpositionActive && (
        <div className="style-detail">
          {(() => {
            const style = THINKING_STYLES.find((s) => s.id === selectedId);
            if (!style) return null;
            return (
              <>
                <div className="style-detail-name" style={{ color: style.color }}>
                  {style.name}
                </div>
                <div className="style-detail-desc">{style.description}</div>
                <div className="style-detail-axis">
                  Axis: <strong>{style.axis}</strong> &middot; Cluster: <strong>{style.cluster}</strong>
                </div>
              </>
            );
          })()}
        </div>
      )}
    </div>
  );
}
