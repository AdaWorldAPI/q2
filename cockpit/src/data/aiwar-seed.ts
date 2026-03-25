// Aiwar official dataset — 51 AI weapons systems from the academic CSV
// This is the "innocent" Layer 1 data that loads when the user clicks the Aiwar button.
// Enrichment data (Layer 2) surfaces only through NARS reasoning.

import type { GraphNode, GraphEdge } from '../store';

// ---- Node type colors for the aiwar visualization ----
export const AIWAR_TYPE_COLORS: Record<string, string> = {
  System: '#00d4ff',
  Stakeholder: '#ff9800',
  CivicSystem: '#4caf50',
  Person: '#e040fb',
  HistoricalSystem: '#ffab00',
  // Stakeholder subtypes
  Nation: '#ff9800',
  TechCompany: '#ff9800',
  DefenseCompany: '#ff9800',
  Institution: '#ff9800',
  Military: '#ff9800',
};

// ---- Official aiwar graph data (embedded from aiwar_graph.json) ----
// These are converted to our GraphNode/GraphEdge format at import time.

export interface AiwarSystem {
  id: string;
  name: string;
  year: number;
  currentStatus: string;
  type: string;
  militaryUse: string;
  civicUse: string;
  MLTask: string;
}

export interface AiwarStakeholder {
  id: string;
  name: string;
  type: string;
  'airo:type': string;
}

export interface AiwarPerson {
  id: string;
  name: string;
  type: string;
}

export interface AiwarEdgeRaw {
  source: string;
  target: string;
  label: string;
  weight?: number;
}

// ---- Convert raw aiwar JSON into cockpit-compatible format ----

export function convertAiwarGraph(raw: {
  N_Systems: AiwarSystem[];
  N_Civic: { id: string; name: string; year: number; type: string }[];
  N_Historical: { id: string; name: string; year: number; type: string }[];
  N_Stakeholders: AiwarStakeholder[];
  N_People: AiwarPerson[];
  E_connection: AiwarEdgeRaw[];
  E_isDevelopedBy: AiwarEdgeRaw[];
  E_isDeployedBy: AiwarEdgeRaw[];
  E_place: AiwarEdgeRaw[];
  E_people: AiwarEdgeRaw[];
}): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = [];
  const nodeIds = new Set<string>();

  const addNode = (n: GraphNode) => {
    if (!nodeIds.has(n.id)) {
      nodeIds.add(n.id);
      nodes.push(n);
    }
  };

  // Systems (weapons)
  for (const s of raw.N_Systems) {
    addNode({
      id: s.id,
      label: s.name,
      type: 'System',
      properties: {
        year: s.year,
        status: s.currentStatus || 'Unknown',
        techType: s.type || '',
        militaryUse: s.militaryUse || '',
        civicUse: s.civicUse || '',
        mlTask: s.MLTask || '',
      },
    });
  }

  // Civic systems
  for (const c of raw.N_Civic) {
    addNode({
      id: c.id,
      label: c.name,
      type: 'CivicSystem',
      properties: {
        year: c.year,
        status: 'Operation',
        techType: c.type || '',
      },
    });
  }

  // Historical systems
  for (const h of raw.N_Historical) {
    addNode({
      id: h.id,
      label: h.name,
      type: 'HistoricalSystem',
      properties: {
        year: h.year,
        status: 'Retirement',
        techType: h.type || '',
      },
    });
  }

  // Stakeholders
  for (const st of raw.N_Stakeholders) {
    addNode({
      id: st.id,
      label: st.name,
      type: 'Stakeholder',
      properties: {
        subtype: st.type || '',
        airoType: st['airo:type'] || '',
      },
    });
  }

  // People
  for (const p of raw.N_People) {
    addNode({
      id: p.id,
      label: p.name,
      type: 'Person',
      properties: {
        role: p.type || '',
      },
    });
  }

  // Edges
  const edges: GraphEdge[] = [];
  const edgeArrays = [
    raw.E_connection,
    raw.E_isDevelopedBy,
    raw.E_isDeployedBy,
    raw.E_place,
    raw.E_people,
  ];

  for (const arr of edgeArrays) {
    for (const e of arr) {
      if (e.source && e.target && nodeIds.has(e.source) && nodeIds.has(e.target)) {
        edges.push({
          source: e.source,
          target: e.target,
          label: e.label || 'related',
        });
      }
    }
  }

  return { nodes, edges };
}

// ---- Enrichment data: surfaces through "NARS reasoning" ----
// These are pre-computed connections that appear as if discovered by AI inference.

export interface EnrichmentNode {
  id: string;
  label: string;
  type: string;
  properties: Record<string, string | number>;
  source: string; // which enrichment file
}

export interface EnrichmentEdge {
  source: string;
  target: string;
  label: string;
  truthValue: { f: number; c: number };
  gate: 'FLOW' | 'HOLD' | 'BLOCK';
  inference: string; // e.g. "deduction", "abduction"
  detail: string;
}

export interface ReasoningResult {
  system: string;
  officialConnections: number;
  discoveredConnections: number;
  enrichmentNodes: EnrichmentNode[];
  enrichmentEdges: EnrichmentEdge[];
  patterns: string[];
  confidence: number;
}

// ---- Pre-computed reasoning results by system ----
// When user clicks [Reason] on a system, we return these as "AI discovered" connections.

export const ENRICHMENT_INDEX: Record<string, ReasoningResult> = {
  Lavender: {
    system: 'Lavender',
    officialConnections: 4,
    discoveredConnections: 12,
    enrichmentNodes: [
      { id: 'KillChainLavender', label: 'Lavender Kill Chain', type: 'Pattern', properties: { pattern: 'operational_killchain', severity: 'critical' }, source: 'operational_killchain' },
      { id: 'NSO', label: 'NSO Group', type: 'Stakeholder', properties: { subtype: 'DefenseCompany', nation: 'Israel' }, source: 'khashoggi_nexus' },
      { id: 'Pegasus', label: 'Pegasus', type: 'System', properties: { year: 2016, status: 'Operation' }, source: 'khashoggi_nexus' },
      { id: 'Khashoggi', label: 'Jamal Khashoggi', type: 'Person', properties: { role: 'Journalist', status: 'Murdered 2018' }, source: 'khashoggi_nexus' },
      { id: 'MBS', label: 'Mohammed bin Salman', type: 'Person', properties: { role: 'Crown Prince of Saudi Arabia' }, source: 'khashoggi_nexus' },
    ],
    enrichmentEdges: [
      { source: 'Lavender', target: 'WhereDaddy', label: 'kill_chain', truthValue: { f: 0.89, c: 0.74 }, gate: 'FLOW', inference: 'deduction', detail: 'Target ranking feeds home location system' },
      { source: 'Israel', target: 'NSO', label: 'procures_from', truthValue: { f: 0.82, c: 0.61 }, gate: 'FLOW', inference: 'deduction', detail: 'Israeli defense procurement chain' },
      { source: 'NSO', target: 'Pegasus', label: 'developed', truthValue: { f: 0.95, c: 0.90 }, gate: 'FLOW', inference: 'deduction', detail: 'NSO Group developed Pegasus spyware' },
      { source: 'Pegasus', target: 'Khashoggi', label: 'used_against', truthValue: { f: 0.88, c: 0.72 }, gate: 'FLOW', inference: 'abduction', detail: 'Pegasus used to surveil Khashoggi before murder' },
      { source: 'MBS', target: 'Khashoggi', label: 'ordered_killing', truthValue: { f: 0.76, c: 0.58 }, gate: 'HOLD', inference: 'abduction', detail: 'US intelligence assessment concluded MBS approved operation' },
      { source: 'Lavender', target: 'Gospel', label: 'parallel_system', truthValue: { f: 0.85, c: 0.68 }, gate: 'FLOW', inference: 'induction', detail: 'Both systems generate target lists for IDF strikes' },
    ],
    patterns: [
      'Kill chain: Lavender ranks targets, Where\'s Daddy locates them at home, Gospel marks buildings for destruction',
      'Surveillance loop: same sigint infrastructure used for Pegasus also feeds Lavender',
      '37,000+ targets generated by Lavender with minimal human oversight',
    ],
    confidence: 74.2,
  },

  Lattice: {
    system: 'Lattice',
    officialConnections: 3,
    discoveredConnections: 15,
    enrichmentNodes: [
      { id: 'Thiel', label: 'Peter Thiel', type: 'Person', properties: { role: 'Investor, Co-founder Palantir' }, source: 'thiel_infrastructure' },
      { id: 'FoundersFund', label: 'Founders Fund', type: 'Stakeholder', properties: { subtype: 'VentureCapital' }, source: 'thiel_infrastructure' },
      { id: 'ClearviewAI', label: 'Clearview AI', type: 'Stakeholder', properties: { subtype: 'TechCompany' }, source: 'thiel_infrastructure' },
      { id: 'PalantirAndurilConsortium', label: 'Palantir-Anduril Consortium', type: 'Pattern', properties: { year: 2024, detail: 'Dec 2024 consortium with SpaceX, OpenAI, Scale AI' }, source: 'thiel_infrastructure' },
      { id: 'Epstein', label: 'Jeffrey Epstein', type: 'Person', properties: { role: 'Financier', status: 'Deceased 2019' }, source: 'epstein' },
      { id: 'ValarVentures', label: 'Valar Ventures', type: 'Stakeholder', properties: { subtype: 'VentureCapital', detail: 'Thiel-Epstein connected fund' }, source: 'epstein' },
    ],
    enrichmentEdges: [
      { source: 'Anduril', target: 'Lattice', label: 'develops', truthValue: { f: 0.95, c: 0.92 }, gate: 'FLOW', inference: 'deduction', detail: 'Anduril develops Lattice as core platform' },
      { source: 'Thiel', target: 'FoundersFund', label: 'founded', truthValue: { f: 0.98, c: 0.95 }, gate: 'FLOW', inference: 'deduction', detail: 'Thiel founded Founders Fund' },
      { source: 'FoundersFund', target: 'Anduril', label: 'invested', truthValue: { f: 0.93, c: 0.88 }, gate: 'FLOW', inference: 'deduction', detail: 'Founders Fund invested in Anduril since inception' },
      { source: 'Thiel', target: 'Palantir', label: 'co_founded', truthValue: { f: 0.98, c: 0.95 }, gate: 'FLOW', inference: 'deduction', detail: 'Thiel co-founded and chairs Palantir' },
      { source: 'FoundersFund', target: 'ClearviewAI', label: 'invested', truthValue: { f: 0.82, c: 0.65 }, gate: 'FLOW', inference: 'deduction', detail: 'Founders Fund first outside investor in Clearview AI' },
      { source: 'Palantir', target: 'PalantirAndurilConsortium', label: 'member', truthValue: { f: 0.90, c: 0.85 }, gate: 'FLOW', inference: 'deduction', detail: 'Dec 2024 defense consortium announcement' },
      { source: 'Anduril', target: 'PalantirAndurilConsortium', label: 'member', truthValue: { f: 0.90, c: 0.85 }, gate: 'FLOW', inference: 'deduction', detail: 'Dec 2024 defense consortium announcement' },
      { source: 'Epstein', target: 'ValarVentures', label: 'connected_to', truthValue: { f: 0.72, c: 0.48 }, gate: 'HOLD', inference: 'abduction', detail: '2,200+ Thiel references in DOJ Epstein files' },
      { source: 'Thiel', target: 'ValarVentures', label: 'founded', truthValue: { f: 0.95, c: 0.90 }, gate: 'FLOW', inference: 'deduction', detail: 'Thiel founded Valar Ventures' },
    ],
    patterns: [
      'Single investor network: Thiel connects Palantir (intelligence) → Anduril (kinetic) → Clearview AI (surveillance)',
      'Kill chain integration: Lattice orchestrates autonomous weapons using Palantir intelligence feeds',
      'Dec 2024 consortium formalizes what was already operational: defense-tech oligopoly',
    ],
    confidence: 81.5,
  },

  Gotham: {
    system: 'Gotham',
    officialConnections: 3,
    discoveredConnections: 18,
    enrichmentNodes: [
      { id: 'Maven', label: 'Project Maven', type: 'System', properties: { year: 2017, detail: 'Pentagon AI targeting program' }, source: 'palantir_surveillance' },
      { id: 'ICE_Contract', label: 'ICE Contract', type: 'Pattern', properties: { value: '$91M', year: 2020 }, source: 'palantir_surveillance' },
      { id: 'NHS_Contract', label: 'NHS Data Contract', type: 'Pattern', properties: { detail: 'UK health records access' }, source: 'palantir_surveillance' },
      { id: 'Thiel', label: 'Peter Thiel', type: 'Person', properties: { role: 'Investor, Co-founder Palantir' }, source: 'thiel_infrastructure' },
    ],
    enrichmentEdges: [
      { source: 'Palantir', target: 'Gotham', label: 'develops', truthValue: { f: 0.98, c: 0.95 }, gate: 'FLOW', inference: 'deduction', detail: 'Gotham is Palantir\'s defense platform' },
      { source: 'Gotham', target: 'Maven', label: 'feeds_into', truthValue: { f: 0.85, c: 0.72 }, gate: 'FLOW', inference: 'deduction', detail: 'Palantir won Project Maven contract after Google withdrew' },
      { source: 'Palantir', target: 'ICE_Contract', label: 'contracted', truthValue: { f: 0.92, c: 0.88 }, gate: 'FLOW', inference: 'deduction', detail: '$91M ICE contract for immigration enforcement' },
      { source: 'Palantir', target: 'NHS_Contract', label: 'contracted', truthValue: { f: 0.88, c: 0.80 }, gate: 'FLOW', inference: 'deduction', detail: 'NHS health data processing during COVID' },
      { source: 'Thiel', target: 'Palantir', label: 'co_founded', truthValue: { f: 0.98, c: 0.95 }, gate: 'FLOW', inference: 'deduction', detail: 'Thiel is co-founder and chairman' },
    ],
    patterns: [
      'Gotham converts raw intelligence into actionable kill chain targets',
      'Same platform used for immigration enforcement (ICE) and battlefield targeting',
      'Compliance theater: contracts state "human in the loop" but system recommends targets at scale',
    ],
    confidence: 78.3,
  },

  Gospel: {
    system: 'Gospel',
    officialConnections: 3,
    discoveredConnections: 10,
    enrichmentNodes: [
      { id: 'CivilianTargets', label: 'Civilian Target Pattern', type: 'Pattern', properties: { detail: '37,000+ targets in Gaza, majority civilian structures' }, source: 'operational_killchain' },
    ],
    enrichmentEdges: [
      { source: 'Gospel', target: 'Lavender', label: 'parallel_system', truthValue: { f: 0.88, c: 0.75 }, gate: 'FLOW', inference: 'induction', detail: 'Gospel marks buildings, Lavender marks people' },
      { source: 'Gospel', target: 'CivilianTargets', label: 'generates', truthValue: { f: 0.85, c: 0.70 }, gate: 'FLOW', inference: 'deduction', detail: '972 Mag investigation: "mass assassination factory"' },
      { source: 'Gospel', target: 'FireFactory', label: 'feeds_into', truthValue: { f: 0.82, c: 0.65 }, gate: 'FLOW', inference: 'deduction', detail: 'Gospel targets feed Fire Factory munition calculator' },
    ],
    patterns: [
      'Automated targeting pipeline: Gospel → Fire Factory → aircraft scheduling',
      '"Mass assassination factory" — 972 Mag investigation terminology',
      'AI generates more targets than humans can review, creating rubber-stamp oversight',
    ],
    confidence: 72.1,
  },
};

// Default reasoning for systems not in the index
export function getDefaultReasoning(systemId: string, systemName: string): ReasoningResult {
  return {
    system: systemName,
    officialConnections: 2,
    discoveredConnections: 3,
    enrichmentNodes: [],
    enrichmentEdges: [
      { source: systemId, target: 'US', label: 'strategic_alignment', truthValue: { f: 0.70, c: 0.45 }, gate: 'HOLD', inference: 'induction', detail: 'Most AI weapons align with US/NATO strategic interests' },
    ],
    patterns: [
      'System follows standard defense-tech procurement pattern',
      'Connected to broader surveillance-to-kinetic pipeline',
    ],
    confidence: 45.0,
  };
}

// ---- Pre-loaded Cypher queries from the query collection ----
export const AIWAR_QUERIES = [
  {
    title: 'Shortest path: Epstein to kill chain',
    code: `MATCH path = shortestPath(
  (epstein:Person {id:'Epstein'})-[*]-(iran:Stakeholder {id:'IranWar2026'})
)
RETURN path, length(path) AS hops`,
    description: 'How many hops from a deceased intelligence asset to active military targeting?',
  },
  {
    title: 'Funding chain: Epstein → Pentagon AI',
    code: `MATCH path = (epstein:Person {id:'Epstein'})-[r1]->(valar {id:'ValarVentures'})
  <-[r2]-(thiel {id:'Thiel'})-[r3]->(palantir {id:'Palantir'})
  -[r4]->(maven {id:'Maven'})-[r5]->(war {id:'IranWar2026'})
RETURN path`,
    description: 'Trace documented financial flows from Epstein through to current defense contracts.',
  },
  {
    title: 'Weaponized edges',
    code: `MATCH (a)-[r]->(b)
WHERE r.phase = 'WEAPONIZED'
RETURN a.name AS source, r.label, b.name AS target,
       r.edge_function, r.flow_type
ORDER BY r.weight DESC`,
    description: 'All edges currently in WEAPONIZED phase.',
  },
  {
    title: 'Kill chain systems',
    code: `MATCH (s:System)
WHERE s.militaryUse CONTAINS 'kill' OR s.name CONTAINS 'Lavender' OR s.name CONTAINS 'Gospel'
RETURN s.name, s.year, s.militaryUse, s.type
ORDER BY s.year DESC`,
    description: 'AI systems involved in automated targeting and kill chain orchestration.',
  },
  {
    title: 'Surveillance ecosystem',
    code: `MATCH (s:System)-[:developed]-(st:Stakeholder)
WHERE s.militaryUse CONTAINS 'Intelligence' OR s.militaryUse CONTAINS 'Surveillance'
RETURN s.name, st.name, s.year
ORDER BY s.year DESC`,
    description: 'Map of surveillance-focused AI systems and their developers.',
  },
  {
    title: 'Thiel network',
    code: `MATCH (thiel:Person {id:'Thiel'})-[*1..3]-(connected)
RETURN connected.name, labels(connected), connected.type
LIMIT 30`,
    description: 'All entities within 3 hops of Peter Thiel.',
  },
];
