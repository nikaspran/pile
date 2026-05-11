use serde::{Deserialize, Serialize};

use super::{AppState, DocumentId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub document_id: DocumentId,
    pub preferred_column: Option<usize>,
    pub visible_rows: Option<usize>,
    pub column_selection: bool,
    pub column_selection_anchor_col: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub state: AppState,
    pub panes: Vec<PaneSnapshot>,
    pub active_pane: usize,
}

impl From<&AppState> for SessionSnapshot {
    fn from(state: &AppState) -> Self {
        Self {
            schema_version: 2,
            state: state.clone(),
            panes: vec![],
            active_pane: 0,
        }
    }
}
