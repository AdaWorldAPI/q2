// TODO: replace when crate is transcoded
//! Graph storage with semiring operations.

use std::collections::HashMap;

pub type VertexId = u64;

#[derive(Debug, Clone)]
pub struct Vertex {
    pub id: VertexId,
    pub label: String,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: VertexId,
    pub to: VertexId,
    pub label: String,
    pub weight: f64,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct QueryResultSet {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

/// A property graph with semiring-weighted edges.
#[derive(Debug, Default)]
pub struct Graph {
    vertices: HashMap<VertexId, Vertex>,
    edges: Vec<Edge>,
    next_id: VertexId,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_vertex(&mut self, label: &str) -> VertexId {
        let id = self.next_id;
        self.next_id += 1;
        self.vertices.insert(
            id,
            Vertex {
                id,
                label: label.to_string(),
                properties: HashMap::new(),
            },
        );
        id
    }

    pub fn add_vertex_with_properties(
        &mut self,
        label: &str,
        properties: HashMap<String, String>,
    ) -> VertexId {
        let id = self.next_id;
        self.next_id += 1;
        self.vertices.insert(
            id,
            Vertex {
                id,
                label: label.to_string(),
                properties,
            },
        );
        id
    }

    pub fn add_edge(&mut self, from: VertexId, to: VertexId, label: &str, weight: f64) {
        self.edges.push(Edge {
            from,
            to,
            label: label.to_string(),
            weight,
            properties: HashMap::new(),
        });
    }

    pub fn get_vertex(&self, id: VertexId) -> Option<&Vertex> {
        self.vertices.get(&id)
    }

    pub fn get_edges_from(&self, id: VertexId) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.from == id).collect()
    }

    pub fn get_edges_to(&self, id: VertexId) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.to == id).collect()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Query vertices by label.
    pub fn query(&self, label: &str) -> QueryResultSet {
        // TODO: implement real query engine
        let vertices: Vec<Vertex> = self
            .vertices
            .values()
            .filter(|v| v.label == label)
            .cloned()
            .collect();
        let vertex_ids: Vec<VertexId> = vertices.iter().map(|v| v.id).collect();
        let edges: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| vertex_ids.contains(&e.from) || vertex_ids.contains(&e.to))
            .cloned()
            .collect();
        QueryResultSet { vertices, edges }
    }
}
