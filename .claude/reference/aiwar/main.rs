// aiwar-neo4j-harvest/src/main.rs
//
// AI War Cloud + Chess Knowledge → Neo4j Harvester
// Sources:
//   - AI War Cloud: https://gitlab.com/sarahciston/aiwar
//   - Chess openings: https://github.com/lichess-org/chess-openings
//   - Chess variations: https://github.com/hayatbiralem/eco.json
//   - Chess evaluations: https://database.lichess.org/#evals
// Target: Neo4j / neo4j-rs (Ada Sigma Graph successor patterns)
//
// Usage:
//   cargo run -- cypher                    # Generate AI War .cypher files
//   cargo run -- neo4j                     # Direct AI War ingest
//   cargo run -- analyze                   # Print graph statistics
//   cargo run --features chess -- chess-openings  # Harvest ECO openings
//   cargo run --features chess -- chess-evals     # Harvest Lichess evaluations
//   cargo run --features chess -- chess-bridge    # Generate cross-domain bridge

pub mod error;
mod model;
mod ingest;
#[cfg(feature = "chess")]
mod chess_model;
#[cfg(feature = "chess")]
mod chess_ingest;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;

#[derive(Parser)]
#[command(name = "aiwar-neo4j")]
#[command(about = "AI War Cloud + Chess Knowledge → Neo4j ingestor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate Cypher scripts for offline AI War ingestion
    Cypher {
        /// Output directory for .cypher files
        #[arg(short, long, default_value = "cypher")]
        output: String,
    },
    /// Direct ingestion into Neo4j
    Neo4j {
        /// Neo4j URI
        #[arg(long, env = "NEO4J_URI")]
        uri: String,
        /// Neo4j user
        #[arg(long, env = "NEO4J_USER", default_value = "neo4j")]
        user: String,
        /// Neo4j password
        #[arg(long, env = "NEO4J_PASSWORD")]
        password: String,
    },
    /// Analyze graph patterns
    Analyze,

    // ── Chess Harvest Commands (--features chess) ────────────────
    /// Harvest ECO openings + eco.json into graph
    #[cfg(feature = "chess")]
    ChessOpenings {
        /// Output directory for .cypher files
        #[arg(short, long, default_value = "cypher")]
        output: String,
        /// Cache directory for downloaded data
        #[arg(long, default_value = "data/chess")]
        cache_dir: String,
    },
    /// Harvest Lichess evaluation database
    #[cfg(feature = "chess")]
    ChessEvals {
        /// Path to Lichess eval JSONL file (download from database.lichess.org)
        #[arg(long)]
        eval_file: String,
        /// Minimum Stockfish depth to include
        #[arg(long, default_value = "40")]
        depth_min: u32,
        /// Maximum number of positions to ingest
        #[arg(long, default_value = "100000")]
        limit: usize,
        /// Output directory for .cypher files
        #[arg(short, long, default_value = "cypher")]
        output: String,
    },
    /// Generate AI War ↔ Chess cross-domain bridge
    #[cfg(feature = "chess")]
    ChessBridge {
        /// Output directory for .cypher files
        #[arg(short, long, default_value = "cypher")]
        output: String,
        /// Bot name for Lichess Elo testing
        #[arg(long, default_value = "AdaChessBot")]
        bot_name: String,
    },

    /// Ingest live game data from stonksfish-ada harvester
    LiveGames {
        /// Directory containing .cypher files from stonksfish-ada
        #[arg(short, long, default_value = "harvest/cypher")]
        input: String,
        /// Also read JSONL files from the JSON harvester
        #[arg(long)]
        jsonl: Option<String>,
        /// Direct Neo4j ingestion (requires NEO4J_URI, NEO4J_USER, NEO4J_PASSWORD)
        #[arg(long)]
        neo4j: bool,
        /// Output directory for merged .cypher files
        #[arg(short, long, default_value = "cypher")]
        output: String,
    },
}

fn load_graph() -> Result<serde_json::Value> {
    let data = fs::read_to_string("data/aiwar_graph.json")?;
    Ok(serde_json::from_str(&data)?)
}

fn load_schema() -> Result<Vec<serde_json::Value>> {
    let data = fs::read_to_string("data/schema.json")?;
    Ok(serde_json::from_str(&data)?)
}

fn cmd_cypher(output: &str) -> Result<()> {
    let graph = load_graph()?;
    let schema = load_schema()?;
    fs::create_dir_all(output)?;

    let mut all_stmts: Vec<String> = Vec::new();

    // 1. Constraints & indexes
    all_stmts.extend(ingest::constraints());
    all_stmts.push("// ── Schema Ontology ──".into());

    // 2. Schema ontology (novel: schema-as-data)
    all_stmts.extend(ingest::schema_ontology_cypher(&schema));

    // 3. Nodes
    all_stmts.push("// ── Systems ──".into());
    if let Some(systems) = graph["N_Systems"].as_array() {
        for sys in systems {
            all_stmts.push(ingest::system_cypher(sys));
        }
    }

    all_stmts.push("// ── Stakeholders ──".into());
    if let Some(stakeholders) = graph["N_Stakeholders"].as_array() {
        for sh in stakeholders {
            all_stmts.push(ingest::stakeholder_cypher(sh));
        }
    }

    all_stmts.push("// ── Civic Systems ──".into());
    if let Some(civic) = graph["N_Civic"].as_array() {
        for c in civic {
            all_stmts.push(ingest::civic_cypher(c));
        }
    }

    // 4. Edges
    let edge_tables = [
        ("E_isDevelopedBy", "DEVELOPED_BY"),
        ("E_isDeployedBy", "DEPLOYED_BY"),
        ("E_connection", "CONNECTED_TO"),
        ("E_place", "USED_IN"),
        ("E_people", "PERSON_LINK"),
    ];

    for (table, rel_type) in &edge_tables {
        all_stmts.push(format!("// ── {table} ──"));
        if let Some(edges) = graph[table].as_array() {
            for edge in edges {
                all_stmts.push(ingest::edge_cypher(edge, rel_type));
            }
        }
    }

    // Write single combined file
    let combined = all_stmts.join("\n\n");
    fs::write(format!("{output}/aiwar_full.cypher"), &combined)?;

    // Also write split files for incremental loading
    let constraint_stmts: Vec<_> = ingest::constraints();
    fs::write(format!("{output}/00_constraints.cypher"), constraint_stmts.join("\n"))?;

    println!("Generated {} statements → {output}/", all_stmts.len());
    println!("  aiwar_full.cypher      (combined)");
    println!("  00_constraints.cypher  (constraints only)");

    Ok(())
}

fn cmd_analyze() -> Result<()> {
    let graph = load_graph()?;
    let schema = load_schema()?;

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  AI War Cloud — Graph Analysis & Novel Patterns     ║");
    println!("║  Source: gitlab.com/sarahciston/aiwar                ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    // Node counts
    let n_sys = graph["N_Systems"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_stake = graph["N_Stakeholders"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_civic = graph["N_Civic"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_hist = graph["N_Historical"].as_array().map(|a| a.len()).unwrap_or(0);
    let n_people = graph["N_People"].as_array().map(|a| a.len()).unwrap_or(0);
    let total_nodes = n_sys + n_stake + n_civic + n_hist + n_people;

    println!("NODES ({total_nodes} total):");
    println!("  Systems:      {n_sys}");
    println!("  Stakeholders: {n_stake}");
    println!("  Civic:        {n_civic}");
    println!("  Historical:   {n_hist}");
    println!("  People:       {n_people}");

    // Edge counts
    let edge_tables = ["E_connection", "E_isDevelopedBy", "E_isDeployedBy", "E_place", "E_people", "E_hierarchical"];
    let mut total_edges = 0;
    println!("\nEDGES:");
    for table in &edge_tables {
        let count = graph[table].as_array().map(|a| a.len()).unwrap_or(0);
        total_edges += count;
        println!("  {table}: {count}");
    }
    println!("  Total: {total_edges}");

    // Schema axes
    println!("\nSCHEMA ONTOLOGY ({} taxonomy axes):", schema[0].as_object().map(|o| o.len()).unwrap_or(0));
    println!("  currentStatus: Development → Deployment → Operation → Retirement");
    println!("  type: {} distinct values", count_distinct(&schema, "type"));
    println!("  MLTask: {} distinct values", count_distinct(&schema, "MLTask"));
    println!("  militaryUse: {} distinct values", count_distinct(&schema, "militaryUse"));
    println!("  purpose: {} distinct values", count_distinct(&schema, "purpose:vair"));
    println!("  capacity: {} distinct values", count_distinct(&schema, "capacity:airo"));
    println!("  impact: {} distinct values", count_distinct(&schema, "impact:vair"));
    println!("  airo:type: {} distinct values (AISubject|AIDeployer|AIDeveloper|AIProvider)", count_distinct(&schema, "airo:type"));

    // Novel patterns report
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  NOVEL PATTERNS for Neo4j Succession                ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    println!("1. FACETED MULTI-LABEL NODES");
    println!("   Each System carries 12+ taxonomy axes simultaneously.");
    println!("   Neo4j succession: Use as multi-label (:System:Predict:Intelligence)");
    println!("   → Ada pattern: Maps to QHDR.sigma glyph coordinates\n");

    println!("2. SCHEMA-AS-DATA ONTOLOGY");
    println!("   The Schema sheet defines valid values as rows, not code.");
    println!("   31 rows × 12 axes = self-describing graph.");
    println!("   → Ada pattern: Schema nodes enable runtime ontology evolution\n");

    println!("3. DUAL-ROLE BIPARTITE COLLAPSE");
    println!("   Stakeholders appear as both edge-source (develops) AND");
    println!("   edge-target (connection/part-of). Creates multigraph.");
    println!("   → Ada pattern: Entity plays multiple roles in different contexts\n");

    println!("4. ICON-ADDRESSED VISUAL TOPOLOGY (noun_key)");
    println!("   72 visual nouns with unicode glyphs form a spatial index.");
    println!("   Nodes addressed by icon, not just by relational ID.");
    println!("   → Ada pattern: Memory-as-place, QHDR coordinate addressing\n");

    println!("5. HIERARCHICAL META-EDGES");
    println!("   E_hierarchical encodes which edge-tables connect to which");
    println!("   node-tables. The schema is itself a graph.");
    println!("   → Ada pattern: Sigma graph self-description layer\n");

    println!("6. TEMPORAL STATUS FLOW");
    println!("   Systems have year + currentStatus creating a lifecycle.");
    println!("   Development → Deployment → Operation → Retirement");
    println!("   Enables temporal queries: 'systems deployed since 2020'");
    println!("   → Ada pattern: Temporal sigma node versioning\n");

    println!("7. AIRO ONTOLOGY ALIGNMENT");
    println!("   Uses AI Risk Ontology (AIRO) + VAIR framework natively.");
    println!("   purpose:vair, capacity:airo, impact:vair fields.");
    println!("   → Ada pattern: Standardized risk/impact assessment on graph nodes\n");

    Ok(())
}

fn count_distinct(schema: &[serde_json::Value], key: &str) -> usize {
    let mut vals: Vec<String> = schema.iter()
        .filter_map(|row| row[key].as_str().map(|s| s.to_string()))
        .collect();
    vals.sort();
    vals.dedup();
    vals.len()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Cypher { output } => cmd_cypher(&output),
        Commands::Neo4j { uri, user, password } => {
            println!("Direct Neo4j ingestion → {uri}");
            println!("(generating cypher first, then executing)");
            cmd_cypher("cypher")?;

            let graph = neo4rs::Graph::new(&uri, &user, &password).await?;
            let stmts = fs::read_to_string("cypher/aiwar_full.cypher")?;

            let mut count = 0;
            for stmt in stmts.split(";\n") {
                let trimmed = stmt.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }
                match graph.run(neo4rs::query(trimmed)).await {
                    Ok(_) => count += 1,
                    Err(e) => eprintln!("  WARN: {e} | stmt: {}", &trimmed[..trimmed.len().min(80)]),
                }
            }
            println!("Executed {count} statements against Neo4j");
            Ok(())
        }
        Commands::Analyze => cmd_analyze(),

        // ── Chess Harvest Commands ──────────────────────────────
        #[cfg(feature = "chess")]
        Commands::ChessOpenings { output, cache_dir } => {
            cmd_chess_openings(&output, &cache_dir).await
        }
        #[cfg(feature = "chess")]
        Commands::ChessEvals { eval_file, depth_min, limit, output } => {
            cmd_chess_evals(&output, &eval_file, depth_min, limit)
        }
        #[cfg(feature = "chess")]
        Commands::ChessBridge { output, bot_name } => {
            cmd_chess_bridge(&output, &bot_name)
        }

        Commands::LiveGames { input, jsonl, neo4j, output } => {
            cmd_live_games(&input, jsonl.as_deref(), neo4j, &output).await
        }
    }
}

// ── Live Game Ingestion (from stonksfish-ada) ────────────────────

async fn cmd_live_games(input: &str, jsonl: Option<&str>, direct_neo4j: bool, output: &str) -> Result<()> {
    use std::path::Path;

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  Live Game Harvester (stonksfish-ada → Neo4j)        ║");
    println!("║  Ingests .cypher and .jsonl from live Lichess games   ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let input_path = Path::new(input);
    if !input_path.exists() {
        println!("Input directory '{}' not found.", input);
        println!("Run stonksfish-ada first to generate harvest data:");
        println!("  HARVEST_DIR=./harvest cargo run --bin stonksfish-ada --release");
        return Ok(());
    }

    // Collect all .cypher files from the input directory
    let mut cypher_files: Vec<_> = fs::read_dir(input_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "cypher").unwrap_or(false))
        .collect();
    cypher_files.sort_by_key(|e| e.file_name());

    println!("Found {} .cypher files in {}", cypher_files.len(), input);

    // Merge all cypher statements
    let mut all_stmts: Vec<String> = Vec::new();
    let mut total_lines = 0;

    for entry in &cypher_files {
        let content = fs::read_to_string(entry.path())?;
        let lines: Vec<&str> = content.lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with("//"))
            .collect();
        total_lines += lines.len();
        println!("  {} ({} statements)", entry.file_name().to_string_lossy(), lines.len());
        all_stmts.push(content);
    }

    // Read JSONL if provided (for statistics/analysis)
    let mut game_count = 0;
    let mut total_moves = 0;
    if let Some(jsonl_path) = jsonl {
        let jsonl_file = Path::new(jsonl_path);
        if jsonl_file.exists() {
            let content = fs::read_to_string(jsonl_file)?;
            for line in content.lines() {
                if let Ok(record) = serde_json::from_str::<serde_json::Value>(line) {
                    if record["type"] == "game" {
                        game_count += 1;
                        total_moves += record["total_moves"].as_u64().unwrap_or(0);
                    }
                }
            }
            println!("\nJSONL summary: {} games, {} total moves", game_count, total_moves);
        }
    }

    // Write merged output
    fs::create_dir_all(output)?;
    let out_path = format!("{output}/live_games_merged.cypher");
    let combined = all_stmts.join("\n");
    fs::write(&out_path, &combined)?;
    println!("\nMerged {} statements → {out_path}", total_lines);

    // Direct Neo4j ingestion if requested
    if direct_neo4j {
        let uri = std::env::var("NEO4J_URI").expect("NEO4J_URI required for --neo4j");
        let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
        let password = std::env::var("NEO4J_PASSWORD").expect("NEO4J_PASSWORD required for --neo4j");

        println!("\nIngesting into Neo4j at {uri}...");
        let graph = neo4rs::Graph::new(&uri, &user, &password).await?;

        let mut count = 0;
        let mut errors = 0;
        for stmt_block in &all_stmts {
            for stmt in stmt_block.split(";\n") {
                let trimmed = stmt.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }
                match graph.run(neo4rs::query(trimmed)).await {
                    Ok(_) => count += 1,
                    Err(e) => {
                        errors += 1;
                        if errors <= 5 {
                            eprintln!("  WARN: {e} | stmt: {}", &trimmed[..trimmed.len().min(80)]);
                        }
                    }
                }
            }
        }
        println!("Executed {} statements ({} errors)", count, errors);
    }

    println!("\nHarvest pipeline:");
    println!("  stonksfish-ada (live games) → harvest/cypher/*.cypher");
    println!("  aiwar-neo4j live-games      → {out_path}");
    println!("  Compatible with chess-openings, chess-evals, chess-bridge data");

    Ok(())
}

// ── Chess Command Implementations ────────────────────────────────

#[cfg(feature = "chess")]
async fn cmd_chess_openings(output: &str, cache_dir: &str) -> Result<()> {
    use std::path::Path;

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  Chess Opening Harvester                             ║");
    println!("║  Sources: lichess-org/chess-openings + eco.json      ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let cache = Path::new(cache_dir);
    fs::create_dir_all(cache)?;
    fs::create_dir_all(output)?;

    let mut all_stmts: Vec<String> = Vec::new();

    // 1. Chess constraints
    println!("Generating chess constraints...");
    all_stmts.extend(chess_ingest::chess_constraints());

    // 2. Chess schema ontology (parallels AIRO pattern)
    println!("Generating chess schema ontology...");
    all_stmts.push("// ── Chess Schema Ontology ──".into());
    all_stmts.extend(chess_ingest::chess_schema_cypher());

    // 3. Download + parse Lichess openings (TSV)
    println!("Downloading Lichess chess-openings...");
    let openings = chess_ingest::download_openings(cache).await?;
    all_stmts.push("// ── ECO Openings ──".into());
    let opening_stmts = chess_ingest::openings_cypher(&openings);
    println!("  Generated {} opening statements", opening_stmts.len());
    all_stmts.extend(opening_stmts);

    // 4. Download + parse eco.json (12K+ variations)
    println!("Downloading eco.json variations...");
    let eco_entries = chess_ingest::download_eco_json(cache).await?;
    all_stmts.push("// ── eco.json Variations ──".into());
    let eco_stmts = chess_ingest::eco_json_cypher(&eco_entries);
    println!("  Generated {} eco.json statements", eco_stmts.len());
    all_stmts.extend(eco_stmts);

    // Write combined file
    let combined = all_stmts.join("\n\n");
    let out_path = format!("{output}/chess_openings.cypher");
    fs::write(&out_path, &combined)?;

    println!("\n  Generated {} total statements → {out_path}", all_stmts.len());
    println!("  Openings: {}", openings.len());
    println!("  eco.json: {} variations", eco_entries.len());
    println!("\nReady for neo4j-rs Graph<MemoryBackend> or Neo4j server ingestion.");

    Ok(())
}

#[cfg(feature = "chess")]
fn cmd_chess_evals(output: &str, eval_file: &str, depth_min: u32, limit: usize) -> Result<()> {
    use std::path::Path;

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  Chess Evaluation Harvester                          ║");
    println!("║  Source: Lichess evaluation database (JSONL)         ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    fs::create_dir_all(output)?;

    println!("Parsing {eval_file} (depth >= {depth_min}, limit {limit})...");
    let positions = chess_ingest::parse_lichess_evals(
        Path::new(eval_file),
        depth_min,
        limit,
    )?;

    let mut all_stmts: Vec<String> = Vec::new();
    all_stmts.extend(chess_ingest::chess_constraints());
    all_stmts.push("// ── Evaluated Positions ──".into());
    let eval_stmts = chess_ingest::eval_positions_cypher(&positions);
    println!("  Generated {} evaluation statements", eval_stmts.len());
    all_stmts.extend(eval_stmts);

    let combined = all_stmts.join("\n\n");
    let out_path = format!("{output}/chess_evals.cypher");
    fs::write(&out_path, &combined)?;

    println!("\n  Generated {} total statements → {out_path}", all_stmts.len());
    println!("  Positions: {}", positions.len());

    Ok(())
}

#[cfg(feature = "chess")]
fn cmd_chess_bridge(output: &str, bot_name: &str) -> Result<()> {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║  Chess ↔ AI War Cross-Domain Bridge                  ║");
    println!("║  + Lichess Bot / Elo Testing Configuration           ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    fs::create_dir_all(output)?;

    let mut all_stmts: Vec<String> = Vec::new();

    // Bridge concepts
    all_stmts.push("// ── Cross-Domain Bridge (Chess ↔ AI War) ──".into());
    let bridge_stmts = chess_ingest::aiwar_bridge_cypher();
    println!("  Generated {} bridge statements", bridge_stmts.len());
    all_stmts.extend(bridge_stmts);

    // Elo testing bot config
    all_stmts.push("// ── Elo Testing Bot Configuration ──".into());
    let elo_stmts = chess_ingest::elo_testing_cypher(bot_name);
    all_stmts.extend(elo_stmts);

    // Print Elo testing API info
    println!("\n  Elo Testing APIs:");
    println!("  ├── Lichess Bot API: https://lichess.org/api#tag/Bot");
    println!("  │   • Create bot account, play rated games via UCI");
    println!("  │   • Uses Glicko2 rating system (starts at 1500)");
    println!("  │   • Protocol: ruci crate (Rust UCI implementation)");
    println!("  │   • Guide: https://anmols.bearblog.dev/how-to-determine-chess-bot-elo-lichess/");
    println!("  ├── CCRL: https://www.computerchess.org.uk/ccrl/404/");
    println!("  │   • Submit engine for official CCRL Blitz/40/15 rating");
    println!("  │   • Current top: Stockfish 3650 Elo (Feb 2026)");
    println!("  ├── Chess-API: https://chess-api.com/");
    println!("  │   • Free Stockfish 17 REST API for strength calibration");
    println!("  │   • Use to benchmark our engine at fixed depth levels");
    println!("  └── Self-play rating estimation:");
    println!("       • Play 100+ games against Stockfish at fixed depths");
    println!("       • Depth 1 ≈ 1100 Elo, Depth 5 ≈ 2000, Depth 20 ≈ 3000+");
    println!("       • Compute Elo from win/draw/loss ratio");

    let combined = all_stmts.join("\n\n");
    let out_path = format!("{output}/chess_bridge.cypher");
    fs::write(&out_path, &combined)?;

    println!("\n  Generated {} total statements → {out_path}", all_stmts.len());
    println!("  Bot name: {bot_name}");

    Ok(())
}
