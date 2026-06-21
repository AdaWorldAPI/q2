import { useState, useCallback } from 'react';

interface Inference {
  premise: string;
  conclusion: string;
  truth: { f: number; c: number };
  gate: 'FLOW' | 'HOLD' | 'BLOCK';
}

interface Scenario {
  label: string;
  truth: { f: number; c: number };
  events: string[];
  signals: string[];
}

interface NotebookState {
  observeResult: { events: string[]; nodeCount: number } | null;
  inferResult: { flow: Inference[]; hold: Inference[]; block: Inference[] } | null;
  searchResult: {
    entities: { name: string; type: string; truth: { f: number; c: number } }[];
    edges: { source: string; target: string; label: string; truth: { f: number; c: number } }[];
    sources: string[];
    llmResponse: string;
  } | null;
  projectResult: { scenarios: Scenario[] } | null;
  reviseResult: { before: { f: number; c: number }; after: { f: number; c: number }; cascadeCount: number } | null;
  graphNodeCount: number;
  currentYear: number;
}

// Simulated notebook state — in production, each cell type calls a real API endpoint
export function useNotebook() {
  const [state, setState] = useState<NotebookState>({
    observeResult: null,
    inferResult: null,
    searchResult: null,
    projectResult: null,
    reviseResult: null,
    graphNodeCount: 51,
    currentYear: 2026,
  });

  const observe = useCallback((timeWindow: string) => {
    // Simulated — would call POST /api/notebook/observe
    const events = timeWindow.includes('March') ? [
      'Venezuela capture (Jan 3)',
      'All-In E263 Pentagon dump (Mar 6)',
      'Anduril Lattice 2.0 deployment (Mar 12)',
      'German drone deal with US (Mar 18)',
      'Palantir $1.2B contract extension (Mar 20)',
      'EU AI Act enforcement review (Mar 21)',
      'SenseTime sanction renewal (Mar 22)',
      'DJI military drone ban debate (Mar 24)',
    ] : [
      'Replicator program Phase 2 (Feb 1)',
      'NSO Group acquisition rumors (Feb 12)',
      'UK Palantir NHS renewal (Feb 18)',
      'AUKUS submarine AI integration (Feb 25)',
    ];
    setState((s) => ({
      ...s,
      observeResult: { events, nodeCount: s.graphNodeCount + events.length },
      graphNodeCount: s.graphNodeCount + events.length,
    }));
  }, []);

  const infer = useCallback((_mode: string, _depth: number) => {
    // Simulated — would call POST /api/notebook/infer
    setState((s) => ({
      ...s,
      inferResult: {
        flow: [
          { premise: 'Lattice 2.0 + Replicator', conclusion: 'Autonomous swarm kill chain operational', truth: { f: 0.88, c: 0.72 }, gate: 'FLOW' },
          { premise: 'German drone deal + NATO', conclusion: 'NATO dependency on US defense tech deepens', truth: { f: 0.91, c: 0.81 }, gate: 'FLOW' },
          { premise: 'Palantir contract + EU data', conclusion: 'European sovereign data risk via US platforms', truth: { f: 0.79, c: 0.65 }, gate: 'FLOW' },
        ],
        hold: [
          { premise: 'Venezuela oil + China energy', conclusion: 'China energy denial motive confirmed?', truth: { f: 0.65, c: 0.41 }, gate: 'HOLD' },
          { premise: 'Palantir expansion + EU AI Act', conclusion: 'EU sovereignty risk from US AI platforms?', truth: { f: 0.58, c: 0.38 }, gate: 'HOLD' },
        ],
        block: [
          { premise: 'Speculation', conclusion: 'Insufficient evidence for conclusion', truth: { f: 0.30, c: 0.20 }, gate: 'BLOCK' },
        ],
      },
      graphNodeCount: s.graphNodeCount + 18,
    }));
  }, []);

  const search = useCallback((_query: string, _model: string) => {
    // Simulated — would call POST /api/notebook/search
    setState((s) => ({
      ...s,
      searchResult: {
        entities: [
          { name: 'CNPC (China National Petroleum Corp)', type: 'Organization', truth: { f: 0.50, c: 0.30 } },
          { name: 'Orinoco Belt (300B barrel reserve)', type: 'Resource', truth: { f: 0.80, c: 0.60 } },
        ],
        edges: [
          { source: 'CNPC', target: 'Venezuela', label: 'invested_in (2007)', truth: { f: 0.50, c: 0.30 } },
          { source: 'US_sanctions', target: 'CNPC', label: 'blocked access', truth: { f: 0.70, c: 0.50 } },
        ],
        sources: ['Reuters', 'CSIS', 'Bloomberg'],
        llmResponse: 'CNPC invested $4B in Venezuela\'s Orinoco Belt in 2007...',
      },
      graphNodeCount: s.graphNodeCount + 8,
    }));
  }, []);

  const project = useCallback((_horizon: string, _count: number) => {
    // Simulated — would call POST /api/notebook/project
    setState((s) => ({
      ...s,
      projectResult: {
        scenarios: [
          {
            label: 'ESCALATION',
            truth: { f: 0.72, c: 0.55 },
            events: [
              'Lattice 2.0 operational in Indo-Pacific',
              'China responds with SCS exclusion zone enforcement',
            ],
            signals: ['satellite imagery', 'naval movements'],
          },
          {
            label: 'ECONOMIC PRESSURE',
            truth: { f: 0.61, c: 0.48 },
            events: [
              'CNPC loses Venezuela access permanently',
              'China accelerates African energy partnerships',
            ],
            signals: ['trade data', 'diplomatic visits'],
          },
          {
            label: 'STATUS QUO',
            truth: { f: 0.45, c: 0.33 },
            events: [
              'All parties maintain current positions',
            ],
            signals: ['absence of escalation markers'],
          },
        ],
      },
    }));
  }, []);

  const revise = useCallback((_edgeDesc: string, newF: number, newC: number, _justification: string) => {
    // Simulated — would call POST /api/notebook/revise
    setState((s) => ({
      ...s,
      reviseResult: {
        before: { f: 0.50, c: 0.30 },
        after: { f: (0.50 + newF) / 2, c: (0.30 + newC) / 2 },
        cascadeCount: 3,
      },
    }));
  }, []);

  const addFlow = useCallback(() => {
    setState((s) => ({ ...s, graphNodeCount: s.graphNodeCount + (s.inferResult?.flow.length || 0) }));
  }, []);

  const addSearch = useCallback(() => {
    setState((s) => ({ ...s, graphNodeCount: s.graphNodeCount + (s.searchResult?.entities.length || 0) }));
  }, []);

  return {
    ...state,
    observe,
    infer,
    search,
    project,
    revise,
    addFlow,
    addSearch,
  };
}
