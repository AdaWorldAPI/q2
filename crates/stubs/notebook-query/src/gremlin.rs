//! Minimal Gremlin → Cypher transpiler for the cockpit's traversal subset.
//!
//! The cockpit's GREMLIN cells (e.g. `g.V().hasLabel('System').outE().inV().path()`)
//! previously returned the whole aiwar graph plus planner metadata — no real
//! traversal. This transpiler lowers the supported Gremlin subset to Cypher so
//! the query runs through lance-graph's REAL `CypherQuery` → DataFusion path
//! (see `execute_cypher`). Anything it can't lower returns `None`, and the
//! caller falls back to the previous graph-echo behavior (the demo never breaks).
//!
//! Supported steps: `g.V()`, `hasLabel('L')`, `has('k','v')`, `out('R')`/`out()`,
//! `in('R')`/`in()`, `outE('R')`/`inE('R')` + `inV()`/`outV()`, `both('R')`/`both()`,
//! `values('p')`, `limit(N)`, `count()`, `path()`. Node vars return `id`/`name`.

/// One parsed Gremlin step: name + raw arg list (already stripped of quotes).
struct Step {
    name: String,
    args: Vec<String>,
}

/// Split a Gremlin traversal into `.step(args)` tokens. Returns `None` if the
/// string isn't a `g.V()`/`g.E()` traversal or contains a step we don't tokenize.
fn parse_steps(src: &str) -> Option<Vec<Step>> {
    let s = src.trim().trim_end_matches(';').trim();
    let rest = s.strip_prefix("g.")?;
    let mut steps = Vec::new();
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // read step name up to '('
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'(' {
            i += 1;
        }
        if i >= bytes.len() {
            return None; // no '(' — malformed
        }
        let name = rest[name_start..i].trim_matches('.').trim().to_string();
        // read args up to matching ')'
        i += 1; // skip '('
        let arg_start = i;
        let mut depth = 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                i += 1;
            }
        }
        if depth != 0 {
            return None;
        }
        let arg_str = &rest[arg_start..i];
        i += 1; // skip ')'
        // skip a trailing '.'
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
        }
        let args = if arg_str.trim().is_empty() {
            Vec::new()
        } else {
            arg_str
                .split(',')
                .map(|a| a.trim().trim_matches(['\'', '"']).to_string())
                .collect()
        };
        if name.is_empty() {
            return None;
        }
        steps.push(Step { name, args });
    }
    if steps.is_empty() { None } else { Some(steps) }
}

/// Transpile a Gremlin traversal to Cypher, or `None` if unsupported.
pub fn gremlin_to_cypher(gremlin: &str) -> Option<String> {
    let steps = parse_steps(gremlin)?;
    let mut it = steps.iter();

    // Must start at the vertex set.
    let first = it.next()?;
    if first.name != "V" {
        return None; // edge-first traversals not supported yet
    }

    let mut var_idx = 0usize;
    let fresh = |idx: &mut usize| {
        let v = format!("n{}", *idx);
        *idx += 1;
        v
    };

    let mut cur = fresh(&mut var_idx); // current vertex var
    let mut node_vars = vec![cur.clone()];
    let mut edge_vars: Vec<String> = Vec::new();
    let mut pattern = format!("({cur}");
    let mut node_open = true; // the current node's `(var` is awaiting close
    let mut cur_has_label = false; // did a hasLabel set the current node's label?
    let mut where_clauses: Vec<String> = Vec::new();
    let mut explicit_return: Option<Vec<String>> = None;
    let mut want_path = false;
    let mut limit: Option<u64> = None;

    // Optionally apply an initial id filter: g.V('id') / g.V(123)
    for a in &first.args {
        where_clauses.push(format!("{cur}.id = '{}'", escape(a)));
    }

    // pending edge between outE/inE and inV/outV
    let mut pending_edge: Option<(String, String)> = None; // (edge_var, "[:LABEL]")

    // Close the open node. An untyped node defaults to `:Entity` — the
    // inheritance root — so the planner can bind it to a table. Without a label,
    // an untyped traversal target fails projection ("No field named n1__id").
    let close_node = |pattern: &mut String, node_open: &mut bool, has_label: &mut bool| {
        if *node_open {
            if !*has_label {
                pattern.push_str(":Entity");
            }
            pattern.push(')');
            *node_open = false;
            *has_label = false;
        }
    };

    for step in it {
        match step.name.as_str() {
            "hasLabel" => {
                let label = step.args.first()?;
                if node_open {
                    pattern.push_str(&format!(":{}", sanitize_label(label)));
                    cur_has_label = true;
                } else {
                    return None;
                }
            }
            "has" => {
                if step.args.len() == 2 {
                    where_clauses.push(format!(
                        "{cur}.{} = '{}'",
                        sanitize_ident(&step.args[0]),
                        escape(&step.args[1])
                    ));
                } else {
                    return None;
                }
            }
            "out" | "in" | "both" => {
                close_node(&mut pattern, &mut node_open, &mut cur_has_label);
                let rel = step
                    .args
                    .first()
                    .map(|r| format!(":{}", sanitize_label(r)))
                    .unwrap_or_else(|| ":Edge".to_string());
                let nv = fresh(&mut var_idx);
                let arrow = match step.name.as_str() {
                    "out" => format!("-[{rel}]->("),
                    "in" => format!("<-[{rel}]-("),
                    _ => format!("-[{rel}]-("),
                };
                pattern.push_str(&arrow);
                pattern.push_str(&nv);
                node_open = true;
                cur = nv.clone();
                node_vars.push(nv);
            }
            "outE" | "inE" | "bothE" => {
                close_node(&mut pattern, &mut node_open, &mut cur_has_label);
                let ev = format!("e{}", edge_vars.len());
                let rel = step
                    .args
                    .first()
                    .map(|r| format!(":{}", sanitize_label(r)))
                    .unwrap_or_else(|| ":Edge".to_string());
                let dir = match step.name.as_str() {
                    "outE" => format!("-[{ev}{rel}]->("),
                    "inE" => format!("<-[{ev}{rel}]-("),
                    _ => format!("-[{ev}{rel}]-("),
                };
                pending_edge = Some((ev.clone(), dir));
                edge_vars.push(ev);
            }
            "inV" | "outV" | "otherV" => {
                let (_ev, dir) = pending_edge.take()?;
                let nv = fresh(&mut var_idx);
                pattern.push_str(&dir);
                pattern.push_str(&nv);
                node_open = true;
                cur = nv.clone();
                node_vars.push(nv);
            }
            "values" => {
                let p = step.args.first()?;
                explicit_return = Some(vec![format!("{cur}.{}", sanitize_ident(p))]);
            }
            "count" => {
                explicit_return = Some(vec!["count(*) AS count".to_string()]);
            }
            "path" => {
                want_path = true;
            }
            "limit" => {
                limit = step.args.first().and_then(|n| n.parse::<u64>().ok());
            }
            "dedup" | "fold" | "unfold" | "as" | "select" | "by" => {
                // tolerated no-ops / unsupported-but-skippable for the cockpit subset
            }
            _ => return None, // unknown step: bail to fallback
        }
    }
    close_node(&mut pattern, &mut node_open, &mut cur_has_label);

    // Build RETURN.
    let ret: Vec<String> = if let Some(r) = explicit_return {
        r
    } else if want_path {
        // path() → every node's id + name across the matched pattern
        node_vars
            .iter()
            .flat_map(|v| [format!("{v}.id"), format!("{v}.name")])
            .collect()
    } else {
        // default: the final vertex's id + name
        vec![format!("{cur}.id"), format!("{cur}.name")]
    };

    let mut cypher = format!("MATCH {pattern}");
    if !where_clauses.is_empty() {
        cypher.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
    }
    cypher.push_str(&format!(" RETURN {}", ret.join(", ")));
    if let Some(n) = limit {
        cypher.push_str(&format!(" LIMIT {n}"));
    }
    Some(cypher)
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Labels and rel types are identifiers in Cypher — keep alnum/underscore only.
fn sanitize_label(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn sanitize_ident(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_with_label() {
        assert_eq!(
            gremlin_to_cypher("g.V().hasLabel('System')").as_deref(),
            Some("MATCH (n0:System) RETURN n0.id, n0.name")
        );
    }

    #[test]
    fn the_cockpit_headline_query() {
        // g.V().hasLabel('server').outE().inV().path()
        let c = gremlin_to_cypher("g.V().hasLabel('server').outE().inV().path()").unwrap();
        assert_eq!(
            c,
            "MATCH (n0:server)-[e0:Edge]->(n1:Entity) RETURN n0.id, n0.name, n1.id, n1.name"
        );
    }

    #[test]
    fn out_with_rel_type_and_limit() {
        let c =
            gremlin_to_cypher("g.V().hasLabel('System').out('DEVELOPED_BY').limit(10)").unwrap();
        assert_eq!(
            c,
            "MATCH (n0:System)-[:DEVELOPED_BY]->(n1:Entity) RETURN n1.id, n1.name LIMIT 10"
        );
    }

    #[test]
    fn has_property_filter() {
        let c = gremlin_to_cypher("g.V().hasLabel('Person').has('name','Karp')").unwrap();
        assert_eq!(
            c,
            "MATCH (n0:Person) WHERE n0.name = 'Karp' RETURN n0.id, n0.name"
        );
    }

    #[test]
    fn values_projection() {
        let c = gremlin_to_cypher("g.V().hasLabel('System').values('name')").unwrap();
        assert_eq!(c, "MATCH (n0:System) RETURN n0.name");
    }

    #[test]
    fn count_terminal() {
        let c = gremlin_to_cypher("g.V().hasLabel('Stakeholder').count()").unwrap();
        assert_eq!(c, "MATCH (n0:Stakeholder) RETURN count(*) AS count");
    }

    #[test]
    fn id_seeded_traversal() {
        let c = gremlin_to_cypher("g.V('Maven').out()").unwrap();
        assert_eq!(
            c,
            "MATCH (n0:Entity)-[:Edge]->(n1:Entity) WHERE n0.id = 'Maven' RETURN n1.id, n1.name"
        );
    }

    #[test]
    fn non_gremlin_returns_none() {
        assert_eq!(gremlin_to_cypher("MATCH (n) RETURN n"), None);
        assert_eq!(gremlin_to_cypher("g.addV('x')"), None); // unsupported step → fallback
    }

    #[test]
    fn injection_chars_are_sanitized() {
        // label/rel identifiers strip non-alnum; string args are escaped.
        let c = gremlin_to_cypher("g.V().hasLabel('Sys;DROP').has('k',\"a'b\")").unwrap();
        assert!(c.contains("(n0:SysDROP)"));
        assert!(c.contains("n0.k = 'a\\'b'"));
    }
}
