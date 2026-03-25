import { useEffect, useMemo, useState, useCallback, useRef } from 'react';
import { useStore } from './store';
import { connectSSE } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';
import { NarsPanel } from './components/NarsPanel';
import { useAiwarData } from './hooks/useAiwarData';
import { convertAiwarGraph, ENRICHMENT_INDEX, getDefaultReasoning, type ReasoningResult } from './data/aiwar-seed';

/**
 * PalantirApp — the SAME dense cockpit layout as DemoApp,
 * but loaded with the 221-node aiwar graph instead of seed data.
 * Inspector is replaced with NarsPanel for reasoning capability.
 */
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

  // Connect SSE
  useEffect(() => {
    connectSSE();
  }, []);

  // Load aiwar data into the store on mount
  useEffect(() => {
    if (loadedRef.current) return;
    loadedRef.current = true;

    (async () => {
      try {
        const [graphRes, weaponsRes] = await Promise.all([
          fetch('/aiwar_graph.json'),
          fetch('/aiwar_weapons.json'),
        ]);
        if (!graphRes.ok) throw new Error(`Graph: HTTP ${graphRes.status}`);
        const raw = await graphRes.json();
        const { nodes: gNodes, edges: gEdges } = convertAiwarGraph(raw);
        setGraphData(gNodes, gEdges);
        setLoaded(true);
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : 'Failed to load aiwar data');
        // Keep the default seed data
        setLoaded(true);
      }
    })();
  }, [setGraphData]);

  // Selected node
  const selectedNode = useMemo(
    () => nodes.find((n) => n.id === selectedNodeId) || null,
    [nodes, selectedNodeId],
  );

  // Handle NARS enrichment — add new nodes/edges to the store
  const handleEnrich = useCallback((result: ReasoningResult) => {
    const currentNodeIds = new Set(nodes.map((n) => n.id));
    const newNodes = result.enrichmentNodes
      .filter((n) => !currentNodeIds.has(n.id))
      .map((n) => ({
        id: n.id,
        label: n.label,
        type: n.type,
        properties: n.properties,
      }));
    const newEdges = result.enrichmentEdges.map((e) => ({
      source: e.source,
      target: e.target,
      label: e.label,
    }));

    if (newNodes.length > 0 || newEdges.length > 0) {
      setGraphData(
        [...nodes, ...newNodes],
        [...edges, ...newEdges],
      );
      setEnrichedNodeIds((prev) => {
        const next = new Set(prev);
        newNodes.forEach((n) => next.add(n.id));
        return next;
      });
    }
  }, [nodes, edges, setGraphData]);

  const cellCount = useMemo(() => {
    const running = cells.filter((c) => c.execution_state === 'running').length;
    return { total: cells.length, running };
  }, [cells]);

  // System count for display
  const systemCount = nodes.filter((n) => n.type === 'System').length;
  const enrichedCount = enrichedNodeIds.size;

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
          {systemCount > 0 && (
            <span className="badge good">{systemCount} systems</span>
          )}
          {enrichedCount > 0 && (
            <span className="badge warn">+{enrichedCount} discovered</span>
          )}
          {loadError && (
            <span className="badge hot" title={loadError}>data error</span>
          )}
          <a href="/demo" className="badge" style={{ textDecoration: 'none', cursor: 'pointer' }}>
            infra demo
          </a>
        </div>
      </section>

      {/* Row 2: Left rail / Graph / NARS Inspector */}
      <LeftRail />
      <GraphPanel />
      <NarsPanel
        selectedNode={selectedNode}
        edges={edges}
        onEnrich={handleEnrich}
      />

      {/* Row 3: Result table */}
      <ResultTable />

      {/* Row 4: Notebook cells */}
      <CellStrip />

      {/* Status bar (bottom) */}
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
          <span>NARS + MCP</span>
          <span className="status-sep" />
          <span>localhost:2718</span>
        </div>
      </footer>

      {/* Tooltip container */}
      <div className="tooltip" id="tooltip" />
    </div>
  );
}
