//! End-to-end: Gremlin → Cypher → real lance-graph DataFusion execution.
//!
//! Confirms the wiring runs without panic and reports whether the transpiled
//! query reached real lance-graph execution ("→ Cypher" in the output) or fell
//! back to the graph echo (a lance-graph Cypher-coverage gap — still safe).

use notebook_query::{execute, QueryLanguage};

fn aiwar_data_path() -> Option<&'static str> {
    for p in [
        "/home/user/q2/cockpit/public/aiwar_graph.json",
        "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
    ] {
        if std::path::Path::new(p).exists() {
            return Some(p);
        }
    }
    None
}

#[test]
fn gremlin_has_label_executes_or_falls_back_cleanly() {
    let Some(data) = aiwar_data_path() else {
        eprintln!("aiwar data not found; skipping e2e");
        return;
    };
    // SAFETY: single-threaded test; sets the loader's data path.
    unsafe { std::env::set_var("AIWAR_DATA_PATH", data) };

    let q = "g.V().hasLabel('System').limit(3)";
    let res = execute(q, QueryLanguage::Gremlin).expect("execute returns Ok");

    eprintln!("--- gremlin e2e ---\nquery: {q}\nraw_output:\n{}", res.raw_output);
    assert!(res.graph_json.is_some(), "graph_json should be present");

    if res.raw_output.contains("→ Cypher") {
        eprintln!("RESULT: real lance-graph Gremlin execution CONFIRMED");
    } else {
        eprintln!("RESULT: fell back to graph echo (lance-graph Cypher coverage gap for this shape)");
    }
}

#[test]
fn gremlin_multihop_path_traversal_runs() {
    let Some(data) = aiwar_data_path() else {
        eprintln!("aiwar data not found; skipping e2e");
        return;
    };
    // SAFETY: single-threaded test.
    unsafe { std::env::set_var("AIWAR_DATA_PATH", data) };

    // The cockpit's headline traversal shape (real label that exists in the data).
    let q = "g.V().hasLabel('System').outE().inV().path()";
    let res = execute(q, QueryLanguage::Gremlin).expect("execute returns Ok");
    eprintln!("--- multi-hop ---\nquery: {q}\nraw_output:\n{}", res.raw_output);
    assert!(res.graph_json.is_some(), "graph_json should be present");
    // With the Entity inheritance root, the untyped target binds and the
    // multi-hop pattern reaches real execution (no "n1__id" projection failure).
    assert!(
        res.raw_output.contains("→ Cypher"),
        "multi-hop traversal should reach real execution via the Entity base, got: {}",
        res.raw_output
    );
}

#[test]
fn gremlin_inherited_one_to_many_returns_rows() {
    let Some(data) = aiwar_data_path() else {
        eprintln!("aiwar data not found; skipping");
        return;
    };
    // SAFETY: single-threaded test.
    unsafe { std::env::set_var("AIWAR_DATA_PATH", data) };

    // Stakeholder -DEVELOPED_BY-> {System|Civic|Historical} (94 edges, 1:N).
    // The heterogeneous target resolves through the Entity inheritance root.
    let q = "g.V().hasLabel('Stakeholder').out('DEVELOPED_BY').limit(5)";
    let res = execute(q, QueryLanguage::Gremlin).expect("execute returns Ok");
    eprintln!("--- inherited 1:N traversal ---\n{}", res.raw_output);
    assert!(
        res.raw_output.contains("→ Cypher"),
        "one-to-many traversal over the Entity base should execute, got: {}",
        res.raw_output
    );
}

#[test]
fn gremlin_returns_dictionary_encoded_label_column() {
    let Some(data) = aiwar_data_path() else {
        eprintln!("aiwar data not found; skipping e2e");
        return;
    };
    // SAFETY: single-threaded test.
    unsafe { std::env::set_var("AIWAR_DATA_PATH", data) };

    // `type` is stored as a dictionary (codebook + u16 index) column. Confirm
    // lance-graph executes a RETURN over it — DataFusion resolves the dictionary
    // back to its string value transparently, so the codebook table is queryable.
    let q = "g.V().hasLabel('System').values('type')";
    let res = execute(q, QueryLanguage::Gremlin).expect("execute returns Ok");
    eprintln!("--- dict column ---\nquery: {q}\nraw_output:\n{}", res.raw_output);
    assert!(
        res.raw_output.contains("→ Cypher"),
        "RETURN over a dictionary-encoded codebook column should reach real lance-graph execution, got: {}",
        res.raw_output
    );

    // The codebook must also resolve in the vis-network JSON (get_json_value's
    // Dictionary arm): at least one System node carries a non-empty `type`.
    let gj = res.graph_json.expect("graph_json present");
    let v: serde_json::Value = serde_json::from_str(&gj).expect("graph_json is valid JSON");
    let resolved = v["nodes"]
        .as_array()
        .map(|ns| {
            ns.iter().any(|n| {
                n["properties"]["type"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
            })
        })
        .unwrap_or(false);
    assert!(
        resolved,
        "graph_json should carry a codebook-resolved 'type' property on at least one node"
    );
}

#[test]
fn probe_typed_multihop_resolves() {
    let Some(data) = aiwar_data_path() else {
        eprintln!("aiwar data not found; skipping");
        return;
    };
    // SAFETY: single-threaded test.
    unsafe { std::env::set_var("AIWAR_DATA_PATH", data) };

    // DEVELOPED_BY stores source=Stakeholder, target=System (dominant, 94×).
    // Untyped target `(b)` previously failed planning ("No field named n1__id").
    // Does typing BOTH ends let lance-graph's planner project the target node?
    let q = "MATCH (a:Stakeholder)-[e:DEVELOPED_BY]->(b:System) RETURN a.id, b.id LIMIT 3";
    match execute(q, QueryLanguage::Cypher) {
        Ok(res) => eprintln!("=== TYPED MULTIHOP RESOLVED ===\n{}", res.raw_output),
        Err(e) => panic!("typed-endpoint multi-hop should resolve, got: {e}"),
    }
}
