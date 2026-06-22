import { ObserveCell } from './cells/ObserveCell';
import { InferCell } from './cells/InferCell';
import { SearchCell } from './cells/SearchCell';
import { ProjectCell } from './cells/ProjectCell';
import { ReviseCell } from './cells/ReviseCell';
import { NotebookTimeline } from './NotebookTimeline';
import { ScenarioCompare } from './ScenarioCompare';
import { useNotebook } from '../hooks/useNotebook';

interface ReasoningNotebookProps {
  onBack: () => void;
}

export function ReasoningNotebook({ onBack }: ReasoningNotebookProps) {
  const nb = useNotebook();

  return (
    <div className="reasoning-notebook">
      {/* Top bar */}
      <div className="aiwar-topbar">
        <div className="aiwar-topbar-left">
          <button className="aiwar-back" onClick={onBack}>&larr; Back</button>
          <h2>Q2 &rsaquo; Reasoning Notebook</h2>
          <span className="badge">Graph: 51 &rarr; {nb.graphNodeCount} nodes</span>
          <span className="badge">Timeline: 2003 &mdash; 2026-W14</span>
        </div>
      </div>

      {/* Timeline */}
      <NotebookTimeline
        startYear={2003}
        endYear={2026}
        currentYear={nb.currentYear}
        nodeCount={nb.graphNodeCount}
        onYearChange={() => {}}
      />

      {/* Cell toolbar */}
      <div className="nb-toolbar">
        <span className="nb-toolbar-label">Add cell:</span>
        <button className="nb-add-btn nb-add-btn--observe">+ OBSERVE</button>
        <button className="nb-add-btn nb-add-btn--infer">+ INFER</button>
        <button className="nb-add-btn nb-add-btn--search">+ SEARCH</button>
        <button className="nb-add-btn nb-add-btn--project">+ PROJECT</button>
        <button className="nb-add-btn nb-add-btn--revise">+ REVISE</button>
      </div>

      {/* Notebook cells */}
      <div className="nb-cells">
        <ObserveCell
          onExecute={nb.observe}
          result={nb.observeResult}
        />

        <InferCell
          onExecute={nb.infer}
          result={nb.inferResult}
          onAddFlow={nb.addFlow}
        />

        <SearchCell
          onExecute={nb.search}
          result={nb.searchResult}
          onAddToGraph={nb.addSearch}
        />

        <ProjectCell
          onExecute={nb.project}
          result={nb.projectResult}
        />

        <ReviseCell
          onExecute={nb.revise}
          result={nb.reviseResult}
        />
      </div>

      {/* Scenario comparison (shows after PROJECT runs) */}
      {nb.projectResult && (
        <ScenarioCompare scenarios={nb.projectResult.scenarios} />
      )}
    </div>
  );
}
