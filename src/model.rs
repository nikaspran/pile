use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type DocumentId = Uuid;

mod app_state;
mod document;
mod session;
mod undo;

pub(crate) use app_state::deserialize_recent_order;
pub use app_state::{AppState, ClosedDocument};
pub use document::{Document, DocumentEdit};
pub use session::{PaneSnapshot, SessionSnapshot};
pub use undo::{EditTransaction, UndoState};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(byte_offset: usize) -> Self {
        Self {
            anchor: byte_offset,
            head: byte_offset,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ScrollState {
    pub x: f32,
    pub y: f32,
}

#[cfg(test)]
mod tests;
