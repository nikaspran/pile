use crop::{Rope, RopeSlice};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::syntax::{LanguageDetection, LanguageId};

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
        self.document(self.active_document)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.document_mut(self.active_document)
    }

    pub fn document(&self, document_id: DocumentId) -> Option<&Document> {
        self.documents
            .iter()
            .find(|document| document.id == document_id)
    }

    pub fn document_mut(&mut self, document_id: DocumentId) -> Option<&mut Document> {
        self.documents
            .iter_mut()
            .find(|document| document.id == document_id)
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

    pub fn set_active(&mut self, document_id: DocumentId) -> bool {
        if self.tab_order.contains(&document_id) && self.document(document_id).is_some() {
            self.active_document = document_id;
            true
        } else {
            false
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
    #[serde(skip)]
    pub occurrence_selections: Vec<Selection>,
    #[serde(skip)]
    pub multi_cursor_query: Option<String>,
    #[serde(skip, default = "UndoState::default")]
    undo: UndoState,
    /// Tab width in spaces (default 4)
    pub tab_width: usize,
    /// Whether to use soft tabs (spaces) instead of tab characters
    pub use_soft_tabs: bool,
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
            occurrence_selections: Vec::new(),
            multi_cursor_query: None,
            undo: UndoState::default(),
            tab_width: 4,
            use_soft_tabs: true,
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

    pub fn detect_syntax(&self) -> Option<LanguageDetection> {
        let registry = crate::syntax::LanguageRegistry;
        Some(registry.detect_rope(&self.rope))
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

    pub fn apply_edit(&mut self, edit: DocumentEdit) {
        let deleted_text = self.rope.byte_slice(edit.range.clone()).to_string();
        self.undo.record(EditTransaction {
            start: edit.range.start,
            end: edit.range.end,
            deleted_text,
            inserted_text: edit.inserted_text.clone(),
            selections_before: edit.selections_before,
        });

        if edit.range.start != edit.range.end {
            self.rope.delete(edit.range.clone());
        }
        if !edit.inserted_text.is_empty() {
            self.rope.insert(edit.range.start, &edit.inserted_text);
        }

        self.selections = edit.selections_after;
        self.revision += 1;
    }

    pub fn apply_grouped_edit(&mut self, edit: DocumentEdit) {
        self.undo.commit_and_start_new_group();
        self.apply_edit(edit);
        self.undo.commit_group();
    }

    pub fn apply_continuing_edit(&mut self, edit: DocumentEdit) {
        self.undo.begin_group();
        self.apply_edit(edit);
    }

    pub fn apply_multi_edit(&mut self, edits: Vec<DocumentEdit>) {
        if edits.is_empty() {
            return;
        }

        let mut transactions = Vec::new();
        let mut offset: isize = 0;

        // Apply edits from last to first to preserve positions.
        // Each edit's range was computed in the original document coordinates.
        // As we apply edits from the end of the document backwards, we accumulate
        // an offset that tells us how much the document has shifted for earlier edits.
        for edit in edits.iter().rev() {
            let adjusted_start = (edit.range.start as isize + offset) as usize;
            let adjusted_end = (edit.range.end as isize + offset) as usize;

            let deleted_text = self.rope.byte_slice(adjusted_start..adjusted_end).to_string();

            // Store the ADJUSTED position (where edit was actually applied).
            // This is the correct position to use during undo.
            transactions.push(EditTransaction {
                start: adjusted_start,
                end: adjusted_end,
                deleted_text,
                inserted_text: edit.inserted_text.clone(),
                selections_before: edit.selections_before.clone(),
            });

            // Apply the edit
            if adjusted_start != adjusted_end {
                self.rope.delete(adjusted_start..adjusted_end);
            }
            if !edit.inserted_text.is_empty() {
                self.rope.insert(adjusted_start, &edit.inserted_text);
            }

            // Update offset: the document has changed by (inserted - deleted) bytes
            offset += edit.inserted_text.len() as isize
                - (edit.range.end as isize - edit.range.start as isize);
        }

        // Transactions are already in application order (last edit first in the vec).
        self.undo.record_multi(transactions);
        self.selections = edits.last().unwrap().selections_after.clone();
        self.revision += 1;
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

        // Undo transactions in REVERSE order (last applied = first undone).
        // Each transaction stores the ADJUSTED position (where the edit was
        // actually applied). Since we undo in reverse order, each undo step
        // returns us to the document state when that edit was applied,
        // so the stored position is valid.
        for txn in group.iter().rev() {
            self.rope
                .delete(txn.start..txn.start + txn.inserted_text.len());
            self.rope.insert(txn.start, &txn.deleted_text);
        }
        if let Some(txn) = group.last() {
            self.selections = txn.selections_before.clone();
        }
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(group) = self.undo.redo() else {
            return false;
        };
        let group = group.clone();

        // Redo transactions in order (reverse of how they were undone).
        // Since undo() processed transactions in reverse, redo processes in order.
        // The stored position is valid because each redo step returns us
        // to that document state.
        for txn in group.iter() {
            self.rope
                .delete(txn.start..txn.start + txn.deleted_text.len());
            self.rope.insert(txn.start, &txn.inserted_text);
        }
        if let Some(txn) = group.last() {
            let new_caret = txn.start + txn.inserted_text.len();
            self.selections = vec![Selection::caret(new_caret)];
        }
        self.revision += 1;
        true
    }

    pub fn record_full_document_replacement(
        &mut self,
        original_text: String,
        selection_before: Selection,
    ) {
        self.undo.commit_and_start_new_group();
        self.undo.record(EditTransaction {
            start: 0,
            end: original_text.len(),
            deleted_text: original_text,
            inserted_text: self.text(),
            selections_before: vec![selection_before],
        });
        self.undo.commit_group();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentEdit {
    pub range: std::ops::Range<usize>,
    pub inserted_text: String,
    pub selections_before: Vec<Selection>,
    pub selections_after: Vec<Selection>,
}

impl DocumentEdit {
    pub fn replace_selection(
        selection: Selection,
        range: std::ops::Range<usize>,
        text: &str,
    ) -> Self {
        Self {
            selections_before: vec![selection],
            selections_after: vec![Selection::caret(range.start + text.len())],
            range,
            inserted_text: text.to_owned(),
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
    pub selections_before: Vec<Selection>,
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

    pub fn record_multi(&mut self, txns: Vec<EditTransaction>) {
        if txns.is_empty() {
            return;
        }
        if self.is_typing {
            self.typing_group.extend(txns);
        } else {
            self.undo_stack.push(txns);
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
    fn set_active_ignores_unknown_documents() {
        let mut state = AppState::empty();
        let active = state.active_document;

        assert!(!state.set_active(Uuid::new_v4()));
        assert_eq!(state.active_document, active);

        let second = state.open_untitled();
        assert!(state.set_active(active));
        assert_eq!(state.active_document, active);
        assert!(state.set_active(second));
        assert_eq!(state.active_document, second);
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
    fn document_edit_replaces_range_and_records_undo() {
        let mut document = Document::new_untitled(1);
        document.replace_text("hello world");
        document.revision = 0;
        let selection = Selection {
            anchor: 6,
            head: 11,
        };

        document.apply_grouped_edit(DocumentEdit::replace_selection(selection, 6..11, "pile"));

        assert_eq!(document.text(), "hello pile");
        assert_eq!(document.selections, vec![Selection::caret(10)]);
        assert_eq!(document.revision, 1);

        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
        assert_eq!(document.selections, vec![selection]);
    }

    #[test]
    fn continuing_edits_share_undo_group_until_committed() {
        let mut document = Document::new_untitled(1);

        document.apply_continuing_edit(DocumentEdit::replace_selection(
            Selection::caret(0),
            0..0,
            "a",
        ));
        document.apply_continuing_edit(DocumentEdit::replace_selection(
            Selection::caret(1),
            1..1,
            "b",
        ));
        document.commit_undo_group();

        assert_eq!(document.text(), "ab");
        assert!(document.undo());
        assert_eq!(document.text(), "");
    }

    #[test]
    fn full_document_replacement_records_single_undo_step() {
        let mut document = Document::new_untitled(1);
        document.replace_text("one\ntwo");
        document.revision = 0;
        let original = document.text();
        let selection = Selection::caret(0);

        document.rope.delete(0..document.rope.byte_len());
        document.rope.insert(0, "two\none");
        document.record_full_document_replacement(original, selection);
        document.revision += 1;

        assert_eq!(document.text(), "two\none");
        assert!(document.undo());
        assert_eq!(document.text(), "one\ntwo");
        assert_eq!(document.selections, vec![selection]);
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
            selections_before: vec![Selection::caret(0)],
        });
        undo.begin_group();
        undo.record(EditTransaction {
            start: 1,
            end: 1,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            selections_before: vec![Selection::caret(1)],
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
            selections_before: vec![Selection::caret(0)],
        });

        // commit_and_start_new_group for a discrete operation should commit the typing group
        undo.commit_and_start_new_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            selections_before: vec![Selection::caret(0)],
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
            selections_before: vec![Selection::caret(0)],
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
            selections_before: vec![Selection::caret(0)],
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
            selections_before: vec![Selection::caret(0)],
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
            selections_before: vec![Selection::caret(0)],
        });
        undo.commit_group();
        undo.undo();

        undo.clear();
        assert!(!undo.can_undo());
        assert!(!undo.can_redo());
    }

    #[test]
    fn multi_edit_creates_single_undo_group() {
        let mut document = Document::new_untitled(1);
        document.replace_text("hello world");
        document.revision = 0;

        // Simulate multi-cursor: replace "hello" and "world" with "hi" and "there"
        let edits = vec![
            DocumentEdit {
                range: 0..5,
                inserted_text: "hi".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(2)],
            },
            DocumentEdit {
                range: 6..11,
                inserted_text: "there".to_owned(),
                selections_before: vec![Selection::caret(6)],
                selections_after: vec![Selection::caret(8)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "hi there");
        assert_eq!(document.revision, 1);

        // Single undo should undo both changes
        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
        assert_eq!(document.revision, 2);
    }

    #[test]
    fn multi_edit_undo_single_edit() {
        let mut document = Document::new_untitled(1);
        document.replace_text("hello");
        document.revision = 0;

        // Single edit via multi_edit
        let edits = vec![
            DocumentEdit {
                range: 0..5,
                inserted_text: "hi".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(2)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "hi");

        // Undo should work
        assert!(document.undo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn multi_edit_undo_restores_all_selections() {
        let mut document = Document::new_untitled(1);
        document.replace_text("a b c");
        document.revision = 0;

        let sel1 = Selection { anchor: 0, head: 1 };
        let sel2 = Selection { anchor: 2, head: 3 };
        let sel3 = Selection { anchor: 4, head: 5 };

        let edits = vec![
            DocumentEdit {
                range: 0..1,
                inserted_text: "x".to_owned(),
                selections_before: vec![sel1],
                selections_after: vec![Selection::caret(1)],
            },
            DocumentEdit {
                range: 2..3,
                inserted_text: "y".to_owned(),
                selections_before: vec![sel2],
                selections_after: vec![Selection::caret(3)],
            },
            DocumentEdit {
                range: 4..5,
                inserted_text: "z".to_owned(),
                selections_before: vec![sel3],
                selections_after: vec![Selection::caret(5)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "x y z");

        // Undo should restore original selections
        assert!(document.undo());
        assert_eq!(document.text(), "a b c");
        assert_eq!(document.selections.len(), 1);
        assert_eq!(document.selections[0], sel1);
    }
}
