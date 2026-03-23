// ═══════════════════════════════════════════════════════════════
// AIWAR ENRICHMENT: Palantir × Surveillance-Industrial Complex
// v1 — 2026-03-02
// Sources:
//   netzpolitik.org (2025) — Palantir policing/military
//   Tagesspiegel (2025) — German police Palantir adoption
//   Euronews (2025) — Baden-Württemberg Gotham contract
//   business-humanrights.org — Egypt surveillance exports
//   DW (2020) — Egyptian spy in Merkel's BPA
//   Stanford Daily (2026) — Thiel-Epstein DOJ files
//   NBC News (2026) — Epstein tech ties
//   NPR (2025) — Palantir in Trump era
//   Democracy Now (2025) — DOGE master database
//   Fortune (2025) — DOGE era stocks
//   Reuters (2025) — Golden Dome bid
//   Novact (2024) — Mass surveillance Maghreb/Mashreq
//   Dandc.eu — European spyware to Egypt
//   EgyptWatch (2021) — French spyware to Sisi
// ═══════════════════════════════════════════════════════════════
// Extends: aiwar_full.cypher (base graph), aiwar_enrichment_epstein.cypher (behavioral schema)
// Uses same behavioral science schema: receptor, mcclelland, rubicon, node_function, edge properties
// ═══════════════════════════════════════════════════════════════

// ════════════════════════════════════════════
// §1  NEW SYSTEM NODES
// ════════════════════════════════════════════

// ── Palantir Products ──
MERGE (n:System:Operation:MLTask_Sort {id: 'ICM'})
SET n.name = 'Investigative Case Management',
    n.year = 2022,
    n.current_status = 'Operation',
    n.system_type = 'DataManagement, Profiling',
    n.ml_task = 'Sort',
    n.military_use = 'nan',
    n.civic_use = 'Policing, BehaviorEvaluation',
    n.purpose = 'AssessingRiskOfOffending, InformationRetrieval',
    n.capacity = 'Profiling, SensitiveAttributeInference, Geolocation, BiometricCategorisation',
    n.output = 'Decision',
    n.impact = 'PsychologicalHarm, DistortionInHumanBehavior',
    n.image = './assets/noun-software-industry-198331.png',
    n.noun_key = 'surveillance',
    n.note = 'ICE database. 5yr/$95.9M contract 2022. Searches birthplace, visa, race, criminal affiliation, license plate data. Tax data sharing added 2025.';

MERGE (n:System:Operation:MLTask_Predict {id: 'MavenSmartSystem'})
SET n.name = 'Maven Smart System (MSS)',
    n.year = 2025,
    n.current_status = 'Operation',
    n.system_type = 'IntelligentControlSystem, DataManagement',
    n.ml_task = 'Predict',
