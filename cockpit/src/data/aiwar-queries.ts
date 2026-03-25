export const AIWAR_QUERIES = [
  { label: 'All AI Systems', lang: 'cypher', code: "MATCH (s:System) RETURN s.name, s.type, s.currentStatus ORDER BY s.name" },
  { label: 'Systems + Developers', lang: 'cypher', code: "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s.name, st.name LIMIT 25" },
  { label: 'Military AI', lang: 'cypher', code: "MATCH (s:System) WHERE s.militaryUse IS NOT NULL RETURN s.name, s.militaryUse, s.type" },
  { label: 'System connections (2 hops)', lang: 'cypher', code: "MATCH p=(a:System)-[*1..2]-(b:System) RETURN p LIMIT 50" },
  { label: 'Surveillance ecosystem', lang: 'cypher', code: "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) WHERE s.type = 'Surveillance' RETURN s, st" },
  { label: 'Palantir network', lang: 'cypher', code: "MATCH (p {name: 'Palantir'})-[r]-(n) RETURN p, r, n" },
  { label: 'Deployed in conflict zones', lang: 'cypher', code: "MATCH (s:System)-[:DEPLOYED_BY]->(st:Stakeholder) WHERE s.militaryUse IS NOT NULL RETURN s.name, st.name, s.militaryUse" },
];

export const INFRASTRUCTURE_QUERIES = [
  { label: 'All nodes', lang: 'cypher', code: "MATCH (n) RETURN n" },
  { label: 'Server → Gateway → Service', lang: 'cypher', code: "MATCH (s:Server)-[:CALLS]->(g:Gateway)-[:CALLS]->(svc:Service) RETURN s.name, g.name, svc.name" },
  { label: 'High CPU nodes', lang: 'cypher', code: "MATCH (n) WHERE n.cpu > 0.7 RETURN n.name, n.cpu, n.status" },
];
