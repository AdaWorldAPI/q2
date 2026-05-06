import type { WireSceneAct } from '../hooks/useShaderStream';
import { fmt, safeNum, safeStr } from '../diagnostics/safe';

interface SceneBreadcrumbProps {
  scene: WireSceneAct | null;
  cycle: number;
}

export function SceneBreadcrumb({ scene, cycle }: SceneBreadcrumbProps) {
  const safeCycle = safeNum(cycle, 0, 'scene.cycle');
  const act = scene ? safeNum(scene.act, 0, 'scene.act') : 0;
  const total = scene ? safeNum(scene.total, 0, 'scene.total') : 0;
  const confidence = scene ? safeNum(scene.confidence, 0, 'scene.confidence') : 0;
  const name = scene
    ? safeStr(scene.name, 'unknown', 'scene.name')
        .replace('aiwar_enrichment_', '')
        .replace('aiwar_', '')
    : 'idle';
  const preview = scene ? safeStr(scene.cypher_preview, '', 'scene.cypher_preview') : '';

  return (
    <div className="scene-breadcrumb">
      <div className="scene-breadcrumb-inner">
        <span className="scene-label">act</span>
        <span className="scene-act">
          {scene ? `${act} / ${total}` : '— / —'}
        </span>
        <span className="scene-sep" />
        <span className="scene-name" title={preview}>
          {name}
        </span>
        {scene && (
          <>
            <span className="scene-sep" />
            <span className="scene-confidence" style={{
              color: confidence >= 0.8 ? 'var(--green)' : confidence >= 0.65 ? 'var(--yellow)' : 'var(--muted)',
            }}>
              c={fmt(confidence, 2, 'scene.confidence')}
            </span>
          </>
        )}
        <span className="scene-sep" />
        <span className="scene-cycle" style={{ color: 'var(--muted)', fontFamily: 'var(--mono)' }}>
          cycle {safeCycle}
        </span>
      </div>
      {preview && (
        <div className="scene-cypher-preview">
          {preview}
        </div>
      )}
    </div>
  );
}
