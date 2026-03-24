// TODO: replace when crate is transcoded from AdaWorldAPI/marimo
use std::collections::HashMap;

pub type CellId = String;

#[derive(Debug, Clone, Default)]
pub struct Notebook {
    pub cells: Vec<Cell>,
    pub metadata: NotebookMetadata,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub id: CellId,
    pub source: String,
    pub language: Option<String>,
    pub outputs: Vec<CellOutput>,
    pub execution_state: ExecutionState,
}

#[derive(Debug, Clone, Default)]
pub struct NotebookMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum CellOutput {
    Html(String),
    Text(String),
    Error(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Graph {
        html: String,
    },
}

#[derive(Debug, Clone, Default)]
pub enum ExecutionState {
    #[default]
    Idle,
    Queued,
    Running,
    Success,
    Error(String),
    Stale,
}

pub struct Runtime {
    pub notebook: Notebook,
    dag: HashMap<CellId, Vec<CellId>>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            notebook: Notebook::default(),
            dag: HashMap::new(),
        }
    }

    pub fn add_cell(&mut self, cell: Cell) -> &CellId {
        self.notebook.cells.push(cell);
        &self.notebook.cells.last().unwrap().id
    }

    pub fn get_cell(&self, id: &str) -> Option<&Cell> {
        self.notebook.cells.iter().find(|c| c.id == id)
    }

    pub fn get_cell_mut(&mut self, id: &str) -> Option<&mut Cell> {
        self.notebook.cells.iter_mut().find(|c| c.id == id)
    }

    pub fn remove_cell(&mut self, id: &str) -> bool {
        let len = self.notebook.cells.len();
        self.notebook.cells.retain(|c| c.id != id);
        self.notebook.cells.len() < len
    }

    pub fn cells(&self) -> &[Cell] {
        &self.notebook.cells
    }

    pub fn dag(&self) -> &HashMap<CellId, Vec<CellId>> {
        &self.dag
    }

    pub fn execute_cell(&mut self, id: &str) -> Result<CellOutput, String> {
        // TODO: implement real execution
        if let Some(cell) = self.get_cell_mut(id) {
            cell.execution_state = ExecutionState::Success;
            Ok(CellOutput::Text(format!("Executed cell {}", id)))
        } else {
            Err(format!("Cell {} not found", id))
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
