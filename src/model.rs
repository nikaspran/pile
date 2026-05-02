use crop::{Rope, RopeSlice};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

pub type DocumentId = Uuid;

const FALLBACK_TITLE: &str = "Untitled";
const MAX_AUTO_TITLE_CHARS: usize = 48;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub documents: Vec<Document>,
    pub tab_order: Vec<DocumentId>,
    pub active_document: DocumentId,
    pub next_untitled_index: u64,
}

impl AppState {
    pub fn empty() -> Self {
        let document = Document::new_untitled(1);
        let active_document = document.id;

        Self {
            documents: vec![document],
            tab_order: vec![active_document],
            active_document,
            next_untitled_index: 2,
        }
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.documents
            .iter()
            .find(|document| document.id == self.active_document)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.documents
            .iter_mut()
            .find(|document| document.id == self.active_document)
    }

    pub fn open_untitled(&mut self) -> DocumentId {
        let index = self.next_untitled_index;
        self.next_untitled_index += 1;

        let document = Document::new_untitled(index);
        let id = document.id;

        self.documents.push(document);
        self.tab_order.push(id);
        self.active_document = id;
        id
    }

    pub fn close_active(&mut self) {
        if self.documents.len() <= 1 {
            if let Some(document) = self.active_document_mut() {
                document.replace_text("");
            }
            return;
        }

        let old_active = self.active_document;
        self.documents.retain(|document| document.id != old_active);
        self.tab_order.retain(|id| *id != old_active);
        self.active_document = self.tab_order.first().copied().unwrap_or_else(|| {
            let document = Document::new_untitled(self.next_untitled_index);
            let id = document.id;
            self.next_untitled_index += 1;
            self.documents.push(document);
            self.tab_order.push(id);
            id
        });
    }

    pub fn set_active(&mut self, document_id: DocumentId) {
        if self.tab_order.contains(&document_id) {
            self.active_document = document_id;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub title_hint: String,
    #[serde(with = "rope_serde")]
    pub rope: Rope,
    pub revision: u64,
    pub selections: Vec<Selection>,
    pub scroll: ScrollState,
    #[serde(skip, default = "UndoState::default")]
    undo: UndoState,
}

mod rope_serde {
    use super::*;

    pub fn serialize<S>(rope: &Rope, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&rope.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Rope, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Ok(Rope::from(text))
    }
}

impl Document {
    pub fn new_untitled(_index: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            title_hint: String::new(),
            rope: Rope::from(""),
            revision: 0,
            selections: vec![Selection::caret(0)],
            scroll: ScrollState::default(),
            undo: UndoState::default(),
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn replace_text(&mut self, text: &str) {
        self.rope = Rope::from(text);
        self.revision += 1;
        self.undo.clear();
    }

    pub fn display_title(&self) -> String {
        if self.has_manual_title() {
            return self.title_hint.clone();
        }

        self.rope
            .lines()
            .find_map(title_from_line)
            .unwrap_or_else(|| FALLBACK_TITLE.to_owned())
    }

    pub fn rename(&mut self, title: &str) {
        self.title_hint = title.trim().to_owned();
    }

    pub fn has_manual_title(&self) -> bool {
        let trimmed = self.title_hint.trim();
        !trimmed.is_empty() && !is_generated_title_hint(trimmed)
    }

    pub fn begin_undo_group(&mut self) {
        self.undo.begin_group();
    }

    pub fn commit_undo_group(&mut self) {
        self.undo.commit_group();
    }

    pub fn discard_undo_group(&mut self) {
        self.undo.discard_group();
    }

    pub fn commit_and_start_new_undo_group(&mut self) {
        self.undo.commit_and_start_new_group();
    }

    pub fn push_undo(&mut self, txn: EditTransaction) {
        self.undo.record(txn);
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        let Some(group) = self.undo.undo() else {
            return false;
        };
        let group = group.clone();
        for txn in group.into_iter().rev() {
            self.rope.delete(txn.start..txn.start + txn.inserted_text.len());
            self.rope.insert(txn.start, &txn.deleted_text);
            self.set_selection_without_undo(txn.selection_before);
        }
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(group) = self.undo.redo() else {
            return false;
        };
        let group = group.clone();
        for txn in group.into_iter() {
            self.rope.delete(txn.start..txn.end);
            self.rope.insert(txn.start, &txn.inserted_text);
            let new_caret = txn.start + txn.inserted_text.len();
            self.set_selection_without_undo(Selection::caret(new_caret));
        }
        self.revision += 1;
        true
    }

    fn set_selection_without_undo(&mut self, selection: Selection) {
        let rope_len = self.rope.byte_len();
        let selection = Selection {
            anchor: selection.anchor.min(rope_len),
            head: selection.head.min(rope_len),
        };
        if let Some(primary) = self.selections.first_mut() {
            *primary = selection;
        } else {
            self.selections.push(selection);
        }
    }
}

fn title_from_line(line: RopeSlice<'_>) -> Option<String> {
    let mut chars = line.chars().skip_while(|char| char.is_whitespace());
    let mut title: String = chars.by_ref().take(MAX_AUTO_TITLE_CHARS).collect();

    if title.trim_end().is_empty() {
        return None;
    }

    let truncated = chars.next().is_some();
    title = title.trim_end().to_owned();

    if truncated {
        title.push_str("...");
    }

    Some(title)
}

fn is_generated_title_hint(title: &str) -> bool {
    title
        .strip_prefix("Scratch ")
        .is_some_and(|suffix| suffix.chars().all(|char| char.is_ascii_digit()))
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditTransaction {
    pub start: usize,
    pub end: usize,
    pub deleted_text: String,
    pub inserted_text: String,
    pub selection_before: Selection,
}

#[derive(Clone, Debug, Default)]
pub struct UndoState {
    undo_stack: Vec<Vec<EditTransaction>>,
    redo_stack: Vec<Vec<EditTransaction>>,
    typing_group: Vec<EditTransaction>,
    is_typing: bool,
}

impl UndoState {
    pub fn begin_group(&mut self) {
        if !self.is_typing {
            self.is_typing = true;
            self.typing_group.clear();
        }
    }

    pub fn commit_and_start_new_group(&mut self) {
        if self.is_typing {
            self.is_typing = false;
            if !self.typing_group.is_empty() {
                self.undo_stack.push(std::mem::take(&mut self.typing_group));
                self.redo_stack.clear();
            }
        }
        self.is_typing = true;
        self.typing_group.clear();
    }

    pub fn record(&mut self, txn: EditTransaction) {
        if self.is_typing {
            self.typing_group.push(txn);
        } else {
            self.undo_stack.push(vec![txn]);
            self.redo_stack.clear();
        }
    }

    pub fn commit_group(&mut self) {
        if self.is_typing {
            self.is_typing = false;
            if !self.typing_group.is_empty() {
                self.undo_stack.push(std::mem::take(&mut self.typing_group));
                self.redo_stack.clear();
            }
        }
    }

    pub fn discard_group(&mut self) {
        if self.is_typing {
            self.is_typing = false;
            self.typing_group.clear();
        }
    }

    pub fn undo(&mut self) -> Option<&Vec<EditTransaction>> {
        self.commit_group();
        if let Some(group) = self.undo_stack.pop() {
            self.redo_stack.push(group.clone());
            Some(self.redo_stack.last().unwrap())
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&Vec<EditTransaction>> {
        if let Some(group) = self.redo_stack.pop() {
            self.undo_stack.push(group.clone());
            Some(self.undo_stack.last().unwrap())
        } else {
            None
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || self.is_typing && !self.typing_group.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.typing_group.clear();
        self.is_typing = false;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub state: AppState,
}

impl From<&AppState> for SessionSnapshot {
    fn from(state: &AppState) -> Self {
        Self {
            schema_version: 1,
            state: state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_closes_scratch_documents_without_losing_last_buffer() {
        let mut state = AppState::empty();
        let first = state.active_document;
        let second = state.open_untitled();

        assert_ne!(first, second);
        assert_eq!(state.documents.len(), 2);
        assert_eq!(state.active_document, second);

        state.close_active();

        assert_eq!(state.documents.len(), 1);
        assert_eq!(state.active_document, first);

        state.close_active();

        assert_eq!(state.documents.len(), 1);
        assert_eq!(state.active_document, first);
    }

    #[test]
    fn document_title_tracks_first_non_empty_line_until_renamed() {
        let mut document = Document::new_untitled(1);
        assert_eq!(document.display_title(), "Untitled");

        document.replace_text("\n  First real line  \nSecond line");
        assert_eq!(document.display_title(), "First real line");

        document.rename("Manual title");
        assert_eq!(document.display_title(), "Manual title");

        document.replace_text("Different first line");
        assert_eq!(document.display_title(), "Manual title");

        document.rename("");
        assert_eq!(document.display_title(), "Different first line");
    }

    #[test]
    fn undo_state_groups_typing_into_single_step() {
        let mut undo = UndoState::default();

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            selection_before: Selection::caret(0),
        });
        undo.begin_group();
        undo.record(EditTransaction {
            start: 1,
            end: 1,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            selection_before: Selection::caret(1),
        });
        undo.commit_group();

        let group = undo.undo().unwrap();
        assert_eq!(group.len(), 2);
    }

    #[test]
    fn undo_state_begins_new_group_commits_previous_typing() {
        let mut undo = UndoState::default();

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            selection_before: Selection::caret(0),
        });

        // commit_and_start_new_group for a discrete operation should commit the typing group
        undo.commit_and_start_new_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            selection_before: Selection::caret(0),
        });
        undo.commit_group();

        // Two separate undo steps: "b" and "a"
        assert!(undo.undo().is_some());
        assert!(undo.undo().is_some());
        assert!(undo.undo().is_none());
    }

    #[test]
    fn undo_state_clears_redo_on_new_edit() {
        let mut undo = UndoState::default();

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "hello".to_owned(),
            selection_before: Selection::caret(0),
        });
        undo.commit_group();

        undo.undo();
        assert!(undo.can_redo());

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "world".to_owned(),
            selection_before: Selection::caret(0),
        });
        undo.commit_group();

        assert!(!undo.can_redo());
    }

    #[test]
    fn undo_state_discard_group_clears_pending_typing() {
        let mut undo = UndoState::default();

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            selection_before: Selection::caret(0),
        });

        undo.discard_group();
        assert!(!undo.can_undo());
    }

    #[test]
    fn undo_state_clear_resets_all_stacks() {
        let mut undo = UndoState::default();

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "hello".to_owned(),
            selection_before: Selection::caret(0),
        });
        undo.commit_group();
        undo.undo();

        undo.clear();
        assert!(!undo.can_undo());
        assert!(!undo.can_redo());
    }
}
