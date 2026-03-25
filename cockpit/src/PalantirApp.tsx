import { useEffect, useMemo, useState, useCallback, useRef } from 'react';
import { useStore } from './store';
import { connectSSE } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';
import { NarsPanel } from './components/NarsPanel';
import { StyleSelector } from './components/StyleSelector';
import { SuperpositionView } from './components/SuperpositionView';
import { convertAiwarGraph, type ReasoningResult } from './data/aiwar-seed';
import { computeActivationPattern, type ThinkingStyle, type ActivationPattern } from './data/thinking-styles';

export function PalantirApp() {
  const connected = useStore((s) => s.connected);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const cells = useStore((s) => s.cells);
  const executing = useStore((s) => s.executing);
  const setGraphData = useStore((s) => s.setGraphData);
  const selectedNodeId = useStore((s) => s.selectedNodeId);

  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [enrichedNodeIds, setEnrichedNodeIds] = useState<Set<string>>(new Set());
  const loadedRef = useRef(false);

  // BNN cognitive lens state
  const [activeStyle, setActiveStyle] = useState<ThinkingStyle | null>(null);
  const [superpositionActive, setSuperpositionActive] = useState(false);
  const [activationPattern, setActivationPattern] = useState<ActivationPattern | null>(null);

  useEffect(() => { connectSSE(); }, []);

  // Load aiwar data
  useEffect(() => {
    if (loadedRef.current) return;
    loadedRef.current = true;
    (async () => {
      try {
        const res = await fetch('/aiwar_graph.json');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const raw = await res.json();
        const { nodes: gNodes, edges: gEdges } = convertAiwarGraph(raw);
        setGraphData(gNodes, gEdges);
        setLoaded(true);
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : 'Failed to load');
        setLoaded(true);
      }
    })();
  }, [setGraphData]);

  // Compute BNN activation when style changes
  useEffect(() => {
    if (!activeStyle || nodes.length === 0) {
      setActivationPattern(null);
      return;
    }
    const pattern = computeActivationPattern(nodes, activeStyle);
    setActivationPattern(pattern);
  }, [activeStyle, nodes]);

  const selectedNode = useMemo(
    () => nodes.find((n) => n.id === selectedNodeId) || null,
    [nodes, selectedNodeId],
  );

  const handleEnrich = useCallback((result: ReasoningResult) => {
    const currentNodeIds = new Set(nodes.map((n) => n.id));
    const newNodes = result.enrichmentNodes
      .filter((n) => !currentNodeIds.has(n.id))
      .map((n) => ({ id: n.id, label: n.label, type: n.type, properties: n.properties }));
    const newEdges = result.enrichmentEdges.map((e) => ({
      source: e.source, target: e.target, label: e.label,
    }));
    if (newNodes.length > 0 || newEdges.length > 0) {
      setGraphData([...nodes, ...newNodes], [...edges, ...newEdges]);
      setEnrichedNodeIds((prev) => {
        const next = new Set(prev);
        newNodes.forEach((n) => next.add(n.id));
        return next;
      });
    }
  }, [nodes, edges, setGraphData]);

  const handleStyleSelect = useCallback((style: ThinkingStyle | null) => {
    setActiveStyle(style);
    setSuperpositionActive(false);
  }, []);

  const handleToggleSuperposition = useCallback(() => {
    setSuperpositionActive((v) => !v);
    setActiveStyle(null);
  }, []);

  const cellCount = useMemo(() => {
    const running = cells.filter((c) => c.execution_state === 'running').length;
    return { total: cells.length, running };
  }, [cells]);

  const systemCount = nodes.filter((n) => n.type === 'System').length;
  const enrichedCount = enrichedNodeIds.size;

  // Show which panel in the right sidebar
  const showSuperposition = superpositionActive && nodes.length > 0;

  return (
    <div className="shell">
      {/* Row 1: Top bar */}
      <section className="topbar">
        <div className="brand">
          <small>q2 graph engine</small>
          <h1>AIWAR</h1>
        </div>
        <QueryBar />
        <div className="top-actions">
          <span className={`badge ${connected ? 'good' : ''}`}>
            {connected ? 'mcp /sse live' : 'disconnected'}
          </span>
          {systemCount > 0 && <span className="badge good">{systemCount} systems</span>}
          {enrichedCount > 0 && <span className="badge warn">+{enrichedCount} discovered</span>}
          {activeStyle && (
            <span className="badge" style={{ color: activeStyle.color, borderColor: `${activeStyle.color}40` }}>
              {activeStyle.name}
              {activationPattern && ` ${activationPattern.fireCount}/${nodes.length}`}
            </span>
          )}
          {superpositionActive && <span className="badge" style={{ color: '#fff' }}>36 BRAINS</span>}
          {loadError && <span className="badge hot" title={loadError}>data error</span>}
          <a href="/demo" className="badge" style={{ textDecoration: 'none', cursor: 'pointer' }}>infra demo</a>
        </div>
      </section>

      {/* Row 2: Left rail / Graph / Right panel */}
      <section className="panel left-rail" style={{ display: 'flex', flexDirection: 'column' }}>
        <div className="panel-header">
          <div className="panel-title">
            <h2>Cognitive Lens</h2>
            <span>36 thinking styles &middot; BNN activation</span>
          </div>
        </div>
        <div className="rail-body" style={{ overflow: 'auto' }}>
          <StyleSelector
            selectedId={activeStyle?.id || null}
            onSelect={handleStyleSelect}
            superpositionActive={superpositionActive}
            onToggleSuperposition={handleToggleSuperposition}
          />
        </div>
      </section>

      <GraphPanel />

      {showSuperposition ? (
        <section className="panel nars-panel" style={{ overflow: 'auto' }}>
          <SuperpositionView nodes={nodes} />
        </section>
      ) : (
        <NarsPanel selectedNode={selectedNode} edges={edges} onEnrich={handleEnrich} />
      )}

      {/* Row 3: Result table */}
      <ResultTable />

      {/* Row 4: Notebook cells */}
      <CellStrip />

      {/* Status bar */}
      <footer className="status-bar">
        <div className="status-bar-left">
          <span className={`status-dot ${connected ? 'online' : 'offline'}`} />
          <span>{connected ? 'Connected' : 'Offline'}</span>
          <span className="status-sep" />
          <span>lance-graph v0.4.2</span>
          <span className="status-sep" />
          <span>{nodes.length} vertices &middot; {edges.length} edges</span>
          <span className="status-sep" />
          <span>{loaded ? 'aiwar loaded' : 'loading...'}</span>
        </div>
        <div className="status-bar-right">
          {executing && <span className="status-executing">executing&hellip;</span>}
          <span>Cell [{cellCount.total}] {cellCount.running > 0 ? 'running' : 'idle'}</span>
          <span className="status-sep" />
          <span>{activeStyle ? `BNN: ${activeStyle.name}` : superpositionActive ? 'BNN: 36x superposition' : 'NARS + MCP'}</span>
          <span className="status-sep" />
          <span>localhost:2718</span>
        </div>
      </footer>

      <div className="tooltip" id="tooltip" />
    </div>
  );
}
