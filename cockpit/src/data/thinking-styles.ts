// 36 Thinking Styles — each defines dendrite weights that determine
// which aiwar graph nodes FIRE (activate) vs SUPPRESS (dim) when
// the BNN processes the graph through that cognitive lens.
//
// Maps to ndarray's SubstrateRoute concept:
//   ThinkingStyle → dendrite weights → BNN activation pattern
//   where activation = dot(style_weights, node_properties) > threshold

export interface ThinkingStyle {
  id: string;
  name: string;
  cluster: 'Analytical' | 'Empathic' | 'Creative' | 'Strategic' | 'Critical' | 'Meta';
  axis: string;
  description: string;
  color: string;
  // Dendrite weights: how much this style cares about each property dimension
  // Values: -1.0 (actively suppress) to +1.0 (strongly activate)
  weights: {
    militaryUse: number;
    civicUse: number;
    surveillance: number;
    autonomous: number;
    intelligence: number;
    economic: number;
    regulation: number;
    historical: number;
    human_rights: number;
    tech_innovation: number;
    geopolitical: number;
    supply_chain: number;
  };
}

export const THINKING_STYLES: ThinkingStyle[] = [
  // ═══ ANALYTICAL CLUSTER (6 styles) ═══
  {
    id: 'hawkish_realist', name: 'Hawkish Realist', cluster: 'Analytical', axis: 'power',
    description: 'Military capability, force projection, deterrence calculus',
    color: '#ff4444',
    weights: { militaryUse: 1.0, civicUse: -0.5, surveillance: 0.6, autonomous: 0.9, intelligence: 0.8, economic: 0.3, regulation: -0.7, historical: 0.4, human_rights: -0.8, tech_innovation: 0.5, geopolitical: 0.9, supply_chain: 0.4 },
  },
  {
    id: 'intelligence_analyst', name: 'Intelligence Analyst', cluster: 'Analytical', axis: 'information',
    description: 'SIGINT, HUMINT, data fusion, pattern recognition',
    color: '#ff6644',
    weights: { militaryUse: 0.6, civicUse: -0.2, surveillance: 1.0, autonomous: 0.4, intelligence: 1.0, economic: 0.2, regulation: -0.3, historical: 0.5, human_rights: -0.4, tech_innovation: 0.7, geopolitical: 0.8, supply_chain: 0.3 },
  },
  {
    id: 'systems_theorist', name: 'Systems Theorist', cluster: 'Analytical', axis: 'complexity',
    description: 'Feedback loops, emergent behavior, system dynamics',
    color: '#ff8844',
    weights: { militaryUse: 0.5, civicUse: 0.5, surveillance: 0.5, autonomous: 0.7, intelligence: 0.5, economic: 0.6, regulation: 0.4, historical: 0.7, human_rights: 0.3, tech_innovation: 0.8, geopolitical: 0.6, supply_chain: 0.8 },
  },
  {
    id: 'quantitative', name: 'Quantitative Analyst', cluster: 'Analytical', axis: 'data',
    description: 'Metrics, probabilities, statistical significance',
    color: '#ffaa44',
    weights: { militaryUse: 0.4, civicUse: 0.4, surveillance: 0.5, autonomous: 0.6, intelligence: 0.5, economic: 0.8, regulation: 0.3, historical: 0.6, human_rights: 0.2, tech_innovation: 0.7, geopolitical: 0.5, supply_chain: 0.7 },
  },
  {
    id: 'game_theorist', name: 'Game Theorist', cluster: 'Analytical', axis: 'strategy',
    description: 'Nash equilibria, incentive structures, rational actors',
    color: '#ffcc44',
    weights: { militaryUse: 0.7, civicUse: 0.1, surveillance: 0.4, autonomous: 0.5, intelligence: 0.6, economic: 0.9, regulation: 0.2, historical: 0.5, human_rights: -0.2, tech_innovation: 0.4, geopolitical: 1.0, supply_chain: 0.6 },
  },
  {
    id: 'technologist', name: 'Defense Technologist', cluster: 'Analytical', axis: 'capability',
    description: 'Technical feasibility, integration readiness, performance',
    color: '#ffee44',
    weights: { militaryUse: 0.8, civicUse: 0.3, surveillance: 0.6, autonomous: 1.0, intelligence: 0.5, economic: 0.4, regulation: -0.3, historical: 0.3, human_rights: -0.3, tech_innovation: 1.0, geopolitical: 0.3, supply_chain: 0.5 },
  },

  // ═══ EMPATHIC CLUSTER (6 styles) ═══
  {
    id: 'human_rights', name: 'Human Rights Advocate', cluster: 'Empathic', axis: 'dignity',
    description: 'Civilian harm, proportionality, legal accountability',
    color: '#44ff44',
    weights: { militaryUse: -0.6, civicUse: 0.8, surveillance: -0.7, autonomous: -0.9, intelligence: -0.3, economic: 0.1, regulation: 0.9, historical: 0.6, human_rights: 1.0, tech_innovation: -0.2, geopolitical: -0.3, supply_chain: 0.0 },
  },
  {
    id: 'diplomatic_inst', name: 'Diplomatic Institutionalist', cluster: 'Empathic', axis: 'cooperation',
    description: 'Multilateral norms, treaty compliance, institutional legitimacy',
    color: '#44ff88',
    weights: { militaryUse: -0.3, civicUse: 0.6, surveillance: -0.4, autonomous: -0.5, intelligence: 0.2, economic: 0.5, regulation: 1.0, historical: 0.5, human_rights: 0.7, tech_innovation: 0.3, geopolitical: 0.6, supply_chain: 0.3 },
  },
  {
    id: 'peace_researcher', name: 'Peace Researcher', cluster: 'Empathic', axis: 'conflict_resolution',
    description: 'De-escalation pathways, structural violence, positive peace',
    color: '#44ffcc',
    weights: { militaryUse: -0.9, civicUse: 0.9, surveillance: -0.8, autonomous: -1.0, intelligence: -0.5, economic: 0.4, regulation: 0.8, historical: 0.8, human_rights: 1.0, tech_innovation: 0.1, geopolitical: 0.2, supply_chain: 0.1 },
  },
  {
    id: 'tech_worker', name: 'Tech Worker Ethicist', cluster: 'Empathic', axis: 'agency',
    description: 'Worker complicity, refusal rights, ethical engineering',
    color: '#44ffff',
    weights: { militaryUse: -0.5, civicUse: 0.7, surveillance: -0.6, autonomous: -0.4, intelligence: -0.2, economic: 0.3, regulation: 0.6, historical: 0.4, human_rights: 0.8, tech_innovation: 0.5, geopolitical: -0.1, supply_chain: 0.2 },
  },
  {
    id: 'civilian_impact', name: 'Civilian Impact Assessor', cluster: 'Empathic', axis: 'harm',
    description: 'Collateral damage, displacement, psychological trauma',
    color: '#44ccff',
    weights: { militaryUse: -0.4, civicUse: 0.5, surveillance: -0.5, autonomous: -0.7, intelligence: -0.1, economic: 0.2, regulation: 0.7, historical: 0.7, human_rights: 0.9, tech_innovation: -0.1, geopolitical: 0.1, supply_chain: 0.0 },
  },
  {
    id: 'journalist', name: 'Investigative Journalist', cluster: 'Empathic', axis: 'accountability',
    description: 'Follow the money, expose concealment, public interest',
    color: '#4488ff',
    weights: { militaryUse: 0.3, civicUse: 0.4, surveillance: 0.8, autonomous: 0.3, intelligence: 0.7, economic: 0.9, regulation: 0.5, historical: 0.6, human_rights: 0.6, tech_innovation: 0.4, geopolitical: 0.5, supply_chain: 0.8 },
  },

  // ═══ CREATIVE CLUSTER (6 styles) ═══
  {
    id: 'economic_strategist', name: 'Economic Strategist', cluster: 'Creative', axis: 'resources',
    description: 'Resource control, trade leverage, economic warfare',
    color: '#ff44ff',
    weights: { militaryUse: 0.3, civicUse: 0.2, surveillance: 0.3, autonomous: 0.2, intelligence: 0.4, economic: 1.0, regulation: 0.2, historical: 0.5, human_rights: -0.1, tech_innovation: 0.6, geopolitical: 0.8, supply_chain: 1.0 },
  },
  {
    id: 'tech_entrepreneur', name: 'Tech Entrepreneur', cluster: 'Creative', axis: 'innovation',
    description: 'Market opportunity, disruption, scale, network effects',
    color: '#ff44cc',
    weights: { militaryUse: 0.2, civicUse: 0.6, surveillance: 0.4, autonomous: 0.8, intelligence: 0.3, economic: 0.9, regulation: -0.6, historical: 0.1, human_rights: -0.1, tech_innovation: 1.0, geopolitical: 0.3, supply_chain: 0.5 },
  },
  {
    id: 'futurist', name: 'Futurist', cluster: 'Creative', axis: 'trajectory',
    description: 'Exponential trends, convergence, singularity dynamics',
    color: '#ff4488',
    weights: { militaryUse: 0.4, civicUse: 0.5, surveillance: 0.5, autonomous: 1.0, intelligence: 0.6, economic: 0.5, regulation: -0.2, historical: 0.3, human_rights: 0.2, tech_innovation: 1.0, geopolitical: 0.4, supply_chain: 0.4 },
  },
  {
    id: 'arms_dealer', name: 'Arms Trade Analyst', cluster: 'Creative', axis: 'markets',
    description: 'Defense contracts, export controls, profit margins',
    color: '#cc44ff',
    weights: { militaryUse: 0.9, civicUse: -0.3, surveillance: 0.5, autonomous: 0.7, intelligence: 0.4, economic: 1.0, regulation: -0.4, historical: 0.4, human_rights: -0.5, tech_innovation: 0.6, geopolitical: 0.7, supply_chain: 0.9 },
  },
  {
    id: 'dual_use', name: 'Dual-Use Researcher', cluster: 'Creative', axis: 'transfer',
    description: 'Civilian-military transfer, repurposing, dual-use dilemma',
    color: '#8844ff',
    weights: { militaryUse: 0.5, civicUse: 0.8, surveillance: 0.5, autonomous: 0.6, intelligence: 0.3, economic: 0.5, regulation: 0.4, historical: 0.6, human_rights: 0.3, tech_innovation: 0.9, geopolitical: 0.3, supply_chain: 0.4 },
  },
  {
    id: 'china_watcher', name: 'China Watcher', cluster: 'Creative', axis: 'competition',
    description: 'PLA modernization, tech transfer, Belt & Road, rare earths',
    color: '#4444ff',
    weights: { militaryUse: 0.7, civicUse: 0.2, surveillance: 0.8, autonomous: 0.6, intelligence: 0.7, economic: 0.9, regulation: 0.1, historical: 0.4, human_rights: 0.3, tech_innovation: 0.8, geopolitical: 1.0, supply_chain: 0.9 },
  },

  // ═══ STRATEGIC CLUSTER (6 styles) ═══
  {
    id: 'pentagon_planner', name: 'Pentagon Planner', cluster: 'Strategic', axis: 'readiness',
    description: 'Force structure, interoperability, logistics, C2',
    color: '#44ff44',
    weights: { militaryUse: 1.0, civicUse: -0.4, surveillance: 0.7, autonomous: 0.9, intelligence: 0.8, economic: 0.5, regulation: -0.4, historical: 0.5, human_rights: -0.5, tech_innovation: 0.7, geopolitical: 0.8, supply_chain: 0.7 },
  },
  {
    id: 'nato_strategist', name: 'NATO Strategist', cluster: 'Strategic', axis: 'alliance',
    description: 'Collective defense, burden sharing, interoperability',
    color: '#88ff44',
    weights: { militaryUse: 0.8, civicUse: 0.1, surveillance: 0.5, autonomous: 0.7, intelligence: 0.6, economic: 0.4, regulation: 0.3, historical: 0.5, human_rights: 0.1, tech_innovation: 0.6, geopolitical: 0.9, supply_chain: 0.5 },
  },
  {
    id: 'cyber_warfare', name: 'Cyber Warfare Specialist', cluster: 'Strategic', axis: 'digital',
    description: 'Network attacks, zero-days, information operations',
    color: '#ccff44',
    weights: { militaryUse: 0.7, civicUse: 0.1, surveillance: 0.9, autonomous: 0.5, intelligence: 0.9, economic: 0.4, regulation: -0.2, historical: 0.3, human_rights: -0.3, tech_innovation: 0.9, geopolitical: 0.6, supply_chain: 0.5 },
  },
  {
    id: 'nuclear_strategist', name: 'Nuclear Strategist', cluster: 'Strategic', axis: 'deterrence',
    description: 'MAD, second strike, escalation ladders, strategic stability',
    color: '#ffff44',
    weights: { militaryUse: 0.9, civicUse: -0.6, surveillance: 0.4, autonomous: 0.6, intelligence: 0.7, economic: 0.3, regulation: 0.2, historical: 0.8, human_rights: -0.4, tech_innovation: 0.5, geopolitical: 1.0, supply_chain: 0.3 },
  },
  {
    id: 'counterterror', name: 'Counterterrorism Analyst', cluster: 'Strategic', axis: 'asymmetric',
    description: 'Non-state actors, radicalization, targeted killing',
    color: '#ffcc00',
    weights: { militaryUse: 0.8, civicUse: -0.2, surveillance: 1.0, autonomous: 0.5, intelligence: 1.0, economic: 0.2, regulation: -0.3, historical: 0.5, human_rights: -0.6, tech_innovation: 0.5, geopolitical: 0.6, supply_chain: 0.2 },
  },
  {
    id: 'space_domain', name: 'Space Domain Strategist', cluster: 'Strategic', axis: 'orbital',
    description: 'Satellite constellations, ASAT, space situational awareness',
    color: '#ff9900',
    weights: { militaryUse: 0.7, civicUse: 0.3, surveillance: 0.8, autonomous: 0.6, intelligence: 0.7, economic: 0.5, regulation: 0.1, historical: 0.3, human_rights: -0.1, tech_innovation: 0.9, geopolitical: 0.7, supply_chain: 0.4 },
  },

  // ═══ CRITICAL CLUSTER (6 styles) ═══
  {
    id: 'postcolonial', name: 'Postcolonial Critic', cluster: 'Critical', axis: 'power_structures',
    description: 'Neo-imperialism, extractive economies, epistemic violence',
    color: '#ff4488',
    weights: { militaryUse: -0.4, civicUse: 0.3, surveillance: -0.7, autonomous: -0.5, intelligence: -0.3, economic: 0.7, regulation: 0.4, historical: 1.0, human_rights: 0.8, tech_innovation: -0.2, geopolitical: 0.6, supply_chain: 0.7 },
  },
  {
    id: 'surveillance_critic', name: 'Surveillance Studies Scholar', cluster: 'Critical', axis: 'panopticon',
    description: 'Mass surveillance, privacy erosion, chilling effects',
    color: '#ff44aa',
    weights: { militaryUse: -0.3, civicUse: 0.5, surveillance: -1.0, autonomous: -0.6, intelligence: -0.5, economic: 0.4, regulation: 0.8, historical: 0.6, human_rights: 0.9, tech_innovation: 0.2, geopolitical: 0.2, supply_chain: 0.3 },
  },
  {
    id: 'feminist_security', name: 'Feminist Security Scholar', cluster: 'Critical', axis: 'gender',
    description: 'Gendered violence, militarized masculinity, care ethics',
    color: '#ff44cc',
    weights: { militaryUse: -0.7, civicUse: 0.6, surveillance: -0.6, autonomous: -0.8, intelligence: -0.2, economic: 0.3, regulation: 0.7, historical: 0.7, human_rights: 1.0, tech_innovation: 0.0, geopolitical: 0.1, supply_chain: 0.1 },
  },
  {
    id: 'political_economy', name: 'Political Economist', cluster: 'Critical', axis: 'capital',
    description: 'Military-industrial complex, revolving door, regulatory capture',
    color: '#cc44ff',
    weights: { militaryUse: 0.4, civicUse: 0.2, surveillance: 0.5, autonomous: 0.3, intelligence: 0.5, economic: 1.0, regulation: 0.6, historical: 0.7, human_rights: 0.4, tech_innovation: 0.4, geopolitical: 0.7, supply_chain: 1.0 },
  },
  {
    id: 'media_critic', name: 'Media & Propaganda Analyst', cluster: 'Critical', axis: 'narrative',
    description: 'Information warfare, manufactured consent, narrative control',
    color: '#8844ff',
    weights: { militaryUse: 0.2, civicUse: 0.4, surveillance: 0.7, autonomous: 0.2, intelligence: 0.8, economic: 0.5, regulation: 0.3, historical: 0.6, human_rights: 0.5, tech_innovation: 0.4, geopolitical: 0.5, supply_chain: 0.4 },
  },
  {
    id: 'legal_scholar', name: 'International Law Scholar', cluster: 'Critical', axis: 'legality',
    description: 'IHL compliance, proportionality, distinction principle, accountability',
    color: '#4444ff',
    weights: { militaryUse: -0.2, civicUse: 0.3, surveillance: -0.4, autonomous: -0.6, intelligence: 0.1, economic: 0.2, regulation: 1.0, historical: 0.8, human_rights: 0.9, tech_innovation: 0.1, geopolitical: 0.4, supply_chain: 0.1 },
  },

  // ═══ META CLUSTER (6 styles) ═══
  {
    id: 'historian', name: 'Military Historian', cluster: 'Meta', axis: 'precedent',
    description: 'Historical parallels, cyclical patterns, lessons not learned',
    color: '#888888',
    weights: { militaryUse: 0.5, civicUse: 0.3, surveillance: 0.4, autonomous: 0.4, intelligence: 0.5, economic: 0.5, regulation: 0.4, historical: 1.0, human_rights: 0.5, tech_innovation: 0.4, geopolitical: 0.6, supply_chain: 0.4 },
  },
  {
    id: 'philosopher', name: 'Philosopher of Technology', cluster: 'Meta', axis: 'ontology',
    description: 'What does autonomy mean? When does a weapon become an agent?',
    color: '#aaaaaa',
    weights: { militaryUse: 0.3, civicUse: 0.5, surveillance: 0.3, autonomous: 0.8, intelligence: 0.4, economic: 0.1, regulation: 0.5, historical: 0.7, human_rights: 0.6, tech_innovation: 0.7, geopolitical: 0.2, supply_chain: 0.1 },
  },
  {
    id: 'epistemologist', name: 'Epistemologist', cluster: 'Meta', axis: 'knowledge',
    description: 'What can we know? Classification games, temporal opacity',
    color: '#cccccc',
    weights: { militaryUse: 0.2, civicUse: 0.3, surveillance: 0.5, autonomous: 0.3, intelligence: 0.6, economic: 0.2, regulation: 0.4, historical: 0.8, human_rights: 0.4, tech_innovation: 0.3, geopolitical: 0.3, supply_chain: 0.2 },
  },
  {
    id: 'network_scientist', name: 'Network Scientist', cluster: 'Meta', axis: 'topology',
    description: 'Degree centrality, clustering, small worlds, power laws',
    color: '#dddddd',
    weights: { militaryUse: 0.4, civicUse: 0.4, surveillance: 0.4, autonomous: 0.4, intelligence: 0.4, economic: 0.6, regulation: 0.3, historical: 0.4, human_rights: 0.2, tech_innovation: 0.5, geopolitical: 0.5, supply_chain: 0.7 },
  },
  {
    id: 'pattern_meta', name: 'Pattern Zero (Meta)', cluster: 'Meta', axis: 'structure',
    description: 'The pattern that generates all other patterns — self-reference',
    color: '#ffffff',
    weights: { militaryUse: 0.5, civicUse: 0.5, surveillance: 0.5, autonomous: 0.5, intelligence: 0.5, economic: 0.5, regulation: 0.5, historical: 0.5, human_rights: 0.5, tech_innovation: 0.5, geopolitical: 0.5, supply_chain: 0.5 },
  },
  {
    id: 'adversarial', name: 'Red Team / Adversarial', cluster: 'Meta', axis: 'attack',
    description: 'What would the adversary do? Exploit analysis, worst case',
    color: '#ff0000',
    weights: { militaryUse: 0.8, civicUse: -0.3, surveillance: 0.7, autonomous: 0.8, intelligence: 0.9, economic: 0.6, regulation: -0.5, historical: 0.5, human_rights: -0.6, tech_innovation: 0.7, geopolitical: 0.9, supply_chain: 0.7 },
  },
];

// ═══ CLUSTER COLORS ═══
export const CLUSTER_COLORS: Record<string, string> = {
  Analytical: '#ff8844',
  Empathic: '#44ff88',
  Creative: '#cc44ff',
  Strategic: '#ffcc44',
  Critical: '#ff44aa',
  Meta: '#aaaaaa',
};

// ═══ BNN ACTIVATION ENGINE ═══
// Simulates the binary neural network activation for the frontend.
// In production, this calls ndarray's BNN via the server API.
// The simulation uses the same mathematical model: dot(weights, features) > threshold

export interface NodeActivation {
  nodeId: string;
  score: number;      // -1.0 (fully suppressed) to +1.0 (fully fired)
  fired: boolean;     // score > threshold
  contributions: Record<string, number>; // which weight dimension contributed most
}

export interface ActivationPattern {
  styleId: string;
  activations: Map<string, NodeActivation>;
  fireCount: number;
  suppressCount: number;
}

// Map node properties to the weight dimensions
function extractFeatures(node: { type: string; properties: Record<string, string | number> }): Record<string, number> {
  const props = node.properties;
  const milUse = String(props.militaryUse || '');
  const civUse = String(props.civicUse || '');
  const techType = String(props.techType || '');
  const subtype = String(props.subtype || '');
  const role = String(props.role || '');
  const year = Number(props.year) || 2020;

  return {
    militaryUse: milUse.length > 0 && milUse !== 'NaN' ? 0.8 : (node.type === 'System' ? 0.5 : 0.1),
    civicUse: civUse.length > 0 && civUse !== 'NaN' ? 0.7 : 0.2,
    surveillance: /surveillance|track|monitor|reconn|pegasus|clearview|facial|camera/i.test(milUse + techType + civUse) ? 0.9 : 0.2,
    autonomous: /autonom|drone|swarm|robot|unmanned|lattice|replicator/i.test(milUse + techType) ? 0.9 : 0.15,
    intelligence: /intel|sigint|data|analysis|foundry|gotham|maven/i.test(milUse + techType) ? 0.85 : 0.2,
    economic: subtype === 'TechCompany' || subtype === 'DefenseCompany' || role.includes('Investor') ? 0.8 : (node.type === 'Stakeholder' ? 0.5 : 0.2),
    regulation: /regulation|compliance|act|law|treaty/i.test(milUse + civUse) ? 0.8 : 0.15,
    historical: year < 2015 ? 0.8 : (year < 2020 ? 0.5 : 0.2),
    human_rights: /civilian|harm|target|kill|assassination|surveillance/i.test(milUse) ? 0.8 : 0.15,
    tech_innovation: /AI|deep.learn|neural|LLM|computer.vision|edge.comput/i.test(techType) ? 0.85 : (node.type === 'System' ? 0.5 : 0.2),
    geopolitical: subtype === 'Nation' || subtype === 'Military' || /NATO|DIANA|AUKUS/i.test(String(props.airoType || '')) ? 0.8 : 0.3,
    supply_chain: role.includes('Investor') || role.includes('Owner') || subtype === 'VentureCapital' ? 0.85 : 0.25,
  };
}

// Compute activation for a single node under a single thinking style
function computeActivation(
  node: { id: string; type: string; properties: Record<string, string | number> },
  style: ThinkingStyle,
  threshold: number = 0.15,
): NodeActivation {
  const features = extractFeatures(node);
  const contributions: Record<string, number> = {};
  let score = 0;
  let totalWeight = 0;

  for (const [dim, weight] of Object.entries(style.weights)) {
    const feature = features[dim] || 0;
    const contribution = weight * feature;
    contributions[dim] = contribution;
    score += contribution;
    totalWeight += Math.abs(weight);
  }

  // Normalize to [-1, 1]
  const normalizedScore = totalWeight > 0 ? score / totalWeight : 0;

  return {
    nodeId: node.id,
    score: normalizedScore,
    fired: normalizedScore > threshold,
    contributions,
  };
}

// Compute full activation pattern for one style across all nodes
export function computeActivationPattern(
  nodes: { id: string; label: string; type: string; properties: Record<string, string | number> }[],
  style: ThinkingStyle,
  threshold: number = 0.15,
): ActivationPattern {
  const activations = new Map<string, NodeActivation>();
  let fireCount = 0;
  let suppressCount = 0;

  for (const node of nodes) {
    const activation = computeActivation(node, style, threshold);
    activations.set(node.id, activation);
    if (activation.fired) fireCount++;
    else suppressCount++;
  }

  return { styleId: style.id, activations, fireCount, suppressCount };
}

// ═══ SUPERPOSITION: Bundle all 36 patterns ═══

export interface SuperpositionResult {
  nodeId: string;
  consensusScore: number;   // 0-36: how many styles fire this node
  fireCount: number;
  suppressCount: number;
  category: 'consensus' | 'fault_line' | 'blind_spot' | 'noise';
  firedBy: string[];        // style IDs that fire this node
  suppressedBy: string[];   // style IDs that suppress this node
}

export function computeSuperposition(
  nodes: { id: string; label: string; type: string; properties: Record<string, string | number> }[],
  styles: ThinkingStyle[] = THINKING_STYLES,
  threshold: number = 0.15,
): Map<string, SuperpositionResult> {
  // Compute all 36 patterns
  const patterns = styles.map((s) => computeActivationPattern(nodes, s, threshold));

  const results = new Map<string, SuperpositionResult>();

  for (const node of nodes) {
    let fireCount = 0;
    let suppressCount = 0;
    const firedBy: string[] = [];
    const suppressedBy: string[] = [];

    for (const pattern of patterns) {
      const activation = pattern.activations.get(node.id);
      if (activation?.fired) {
        fireCount++;
        firedBy.push(pattern.styleId);
      } else {
        suppressCount++;
        suppressedBy.push(pattern.styleId);
      }
    }

    const total = styles.length;
    let category: SuperpositionResult['category'];
    if (fireCount >= total * 0.8) category = 'consensus';
    else if (fireCount >= total * 0.3 && fireCount <= total * 0.7) category = 'fault_line';
    else if (fireCount <= total * 0.1) category = 'blind_spot';
    else category = 'noise';

    results.set(node.id, {
      nodeId: node.id,
      consensusScore: fireCount,
      fireCount,
      suppressCount,
      category,
      firedBy,
      suppressedBy,
    });
  }

  return results;
}
