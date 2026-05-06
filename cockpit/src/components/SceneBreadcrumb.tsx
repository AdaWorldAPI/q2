import type { ShaderEvent, WireSceneAct } from '../hooks/useShaderStream';

interface SceneBreadcrumbProps {
  scene: WireSceneAct | null;
  cycle: number;
}

export function SceneBreadcrumb({ scene, cycle }: SceneBreadcrumbProps) {
  return (
    <div className="scene-breadcrumb">
      <div className="scene-breadcrumb-inner">
        <span className="scene-label">act</span>
        <span className="scene-act">
          {scene ? `${scene.act} / ${scene.total}` : '— / —'}
        </span>
        <span className="scene-sep" />
        <span className="scene-name" title={scene?.cypher_preview ?? ''}>
          {scene?.name?.replace('aiwar_enrichment_', '').replace('aiwar_', '') ?? 'idle'}
        </span>
        {scene && (
          <>
            <span className="scene-sep" />
            <span className="scene-confidence" style={{
              color: scene.confidence >= 0.8 ? 'var(--green)' : scene.confidence >= 0.65 ? 'var(--yellow)' : 'var(--muted)',
            }}>
              c={scene.confidence.toFixed(2)}
            </span>
          </>
        )}
        <span className="scene-sep" />
        <span className="scene-cycle" style={{ color: 'var(--muted)', fontFamily: 'var(--mono)' }}>
          cycle {cycle}
        </span>
      </div>
      {scene?.cypher_preview && (
        <div className="scene-cypher-preview">
          {scene.cypher_preview}
        </div>
      )}
    </div>
  );
}
