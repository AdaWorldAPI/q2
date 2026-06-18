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
    // Either real multi-hop execution or a safe fall-back — never a panic/500.
    eprintln!(
        "multi-hop path: {}",
        if res.raw_output.contains("→ Cypher") { "REAL execution" } else { "fallback" }
    );
}
