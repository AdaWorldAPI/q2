/**
 * StyleSelector — 36-brain cognitive style picker for the Reasoning cockpit.
 *
 * Phase 3 Agent #A2. Canonical names + cluster ordering pulled from
 * `lance_graph_contract::thinking::ThinkingStyle`. Six clusters of six
 * styles each, indexed by their tau (τ) macro address:
 *
 *   Exploratory (τ 0x20)   →  Curious, Exploratory, Questioning, Investigative,
 *                             Speculative, Philosophical
 *   Analytical  (τ 0x40)   →  Logical, Analytical, Critical, Systematic,
 *                             Methodical, Precise
 *   Direct      (τ 0x60)   →  Direct, Concise, Efficient, Pragmatic,
 *                             Blunt, Frank
 *   Empathic    (τ 0x80)   →  Empathetic, Compassionate, Supportive,
 *                             Nurturing, Gentle, Warm
 *   Creative    (τ 0xA0)   →  Creative, Imaginative, Innovative, Artistic,
 *                             Poetic, Playful
 *   Meta        (τ 0xC0)   →  Reflective, Contemplative, Metacognitive,
 *                             Wise, Transcendent, Sovereign
 *
 * Click → optimistic local highlight + POST /v1/shader/style. The backend
 * (Phase 3 A3) confirms via the SSE dispatch event; ReasoningPage renders
 * `lastDispatch.style` below the grid so the user sees the round-trip.
 *
 * "Auto" clears the selection — the backend falls back to its automatic
 * style picker.
 */

export interface StyleCluster {
  name: string;
  tau: number;
  styles: string[];
}

/**
 * Canonical 6 × 6 cluster layout. Order chosen for visual ascent of tau:
 * Exploratory (smallest tau) at top → Meta (largest tau) at bottom.
 */
export const STYLE_CLUSTERS: StyleCluster[] = [
  {
    name: 'Exploratory',
    tau: 0x20,
    styles: [
      'Curious',
      'Exploratory',
      'Questioning',
      'Investigative',
      'Speculative',
      'Philosophical',
    ],
  },
  {
    name: 'Analytical',
    tau: 0x40,
    styles: [
      'Logical',
      'Analytical',
      'Critical',
      'Systematic',
      'Methodical',
      'Precise',
    ],
  },
  {
    name: 'Direct',
    tau: 0x60,
    styles: ['Direct', 'Concise', 'Efficient', 'Pragmatic', 'Blunt', 'Frank'],
  },
  {
    name: 'Empathic',
    tau: 0x80,
    styles: [
      'Empathetic',
      'Compassionate',
      'Supportive',
      'Nurturing',
      'Gentle',
      'Warm',
    ],
  },
  {
    name: 'Creative',
    tau: 0xa0,
    styles: [
      'Creative',
      'Imaginative',
      'Innovative',
      'Artistic',
      'Poetic',
      'Playful',
    ],
  },
  {
    name: 'Meta',
    tau: 0xc0,
    styles: [
      'Reflective',
      'Contemplative',
      'Metacognitive',
      'Wise',
      'Transcendent',
      'Sovereign',
    ],
  },
];

/**
 * Cycle-fingerprint hue (kept consistent with EnergyField's tau→hue scheme).
 * Maps a tau byte to a CSS hsl() hue: 0x20 → 220° blue, 0xC0 → 360° red.
 */
function tauToHue(tau: number): number {
  // Tau range used: 0x20..=0xC5 → ~32..197 (165 span).
  // Map linearly into 200°..360° (cyan→magenta→red).
  const t = Math.max(0, Math.min(1, (tau - 0x20) / (0xc5 - 0x20)));
  return 200 + t * 160;
}

export interface StyleSelectorProps {
  /** Currently active style name (canonical, e.g. "Curious"). null → Auto. */
  active: string | null;
  /** Fires when the user picks a style. Caller is responsible for the POST. */
  onSelect: (style: string | null) => void;
  /** Optional: most recent dispatch style echoed back from the SSE stream. */
  lastDispatchStyle?: string | null;
  /** Optional: surface a transient inline error (e.g. last POST returned !ok). */
  errorText?: string | null;
}

export function StyleSelector({
  active,
  onSelect,
  lastDispatchStyle,
  errorText,
}: StyleSelectorProps) {
  // Defensive: if the cluster table somehow shipped empty, render a
  // calm placeholder instead of an empty grid.
  if (!STYLE_CLUSTERS.length) {
    return (
      <div className="style-selector style-selector-loading">
        loading thinking styles…
      </div>
    );
  }

  return (
    <div className="style-selector" role="group" aria-label="thinking style">
      <div className="style-selector-head">
        <span className="style-selector-title">θ thinking style</span>
        <button
          type="button"
          className={`style-btn style-auto ${active === null ? 'active' : ''}`}
          onClick={() => onSelect(null)}
          title="Let the engine pick the style"
        >
          Auto
        </button>
        <span className="style-selector-confirm">
          {lastDispatchStyle ? (
            <>
              dispatch:&nbsp;
              <span className="style-selector-confirm-name">
                {lastDispatchStyle}
              </span>
            </>
          ) : (
            <span className="style-selector-confirm-muted">
              awaiting dispatch
            </span>
          )}
        </span>
        {errorText && (
          <span className="style-selector-error" title={errorText}>
            {errorText}
          </span>
        )}
      </div>

      <div className="style-grid">
        {STYLE_CLUSTERS.map((cluster) => {
          const hue = tauToHue(cluster.tau);
          return (
            <div
              key={cluster.name}
              className="style-row"
              style={{
                background: `hsla(${hue}, 55%, 16%, 0.45)`,
                borderLeft: `2px solid hsla(${hue}, 70%, 55%, 0.7)`,
              }}
            >
              <div
                className="style-cluster-label"
                title={`τ ${cluster.tau.toString(16).padStart(2, '0')}`}
                style={{ color: `hsl(${hue}, 70%, 70%)` }}
              >
                {cluster.name}
                <span className="style-tau">
                  τ{cluster.tau.toString(16).padStart(2, '0')}
                </span>
              </div>
              {cluster.styles.map((style) => {
                const isActive = active === style;
                return (
                  <button
                    type="button"
                    key={style}
                    className={`style-btn ${isActive ? 'active' : ''}`}
                    style={
                      isActive
                        ? {
                            borderColor: `hsl(${hue}, 80%, 60%)`,
                            boxShadow: `0 0 8px hsla(${hue}, 80%, 60%, 0.7)`,
                            background: `hsla(${hue}, 60%, 22%, 0.85)`,
                          }
                        : undefined
                    }
                    onClick={() => onSelect(isActive ? null : style)}
                    title={`${cluster.name} · ${style}`}
                  >
                    {style}
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
