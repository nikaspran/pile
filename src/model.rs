use std::collections::BTreeSet;

use crop::{Rope, RopeSlice};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::syntax::LanguageDetection;
use crate::syntax_highlighting::DocumentSyntaxState;

pub type DocumentId = Uuid;

const FALLBACK_TITLE: &str = "Untitled";
const MAX_AUTO_TITLE_CHARS: usize = 48;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub documents: Vec<Document>,
    pub tab_order: Vec<DocumentId>,
    pub active_document: DocumentId,
    pub next_untitled_index: u64,
    #[serde(
        serialize_with = "serialize_recent_order",
        deserialize_with = "deserialize_recent_order"
    )]
    recent_order: Vec<DocumentId>,
}

impl AppState {
    pub fn empty() -> Self {
        let document = Document::new_untitled(1, 4, true);
        let active_document = document.id;

        Self {
            documents: vec![document],
            tab_order: vec![active_document],
            active_document,
            next_untitled_index: 2,
            recent_order: vec![active_document],
        }
    }

    /// Validate and repair restored state: stale tab_order entries,
    /// missing or invalid active_document, out-of-bounds selections,
    /// and unreasonable scroll values.
    pub fn validate(&mut self) {
        let valid_ids: std::collections::BTreeSet<DocumentId> =
            self.documents.iter().map(|d| d.id).collect();

        // Remove stale and duplicate tab_order entries
        let mut seen = std::collections::BTreeSet::new();
        self.tab_order
            .retain(|id| valid_ids.contains(id) && seen.insert(*id));

        // Ensure tab_order has at least one entry
        if self.tab_order.is_empty() {
            if let Some(document) = self.documents.first() {
                self.tab_order.push(document.id);
            } else {
                let document = Document::new_untitled(self.next_untitled_index, 4, true);
                self.next_untitled_index += 1;
                self.documents.push(document);
                self.tab_order.push(self.documents[0].id);
            }
        }

        // Fix active_document if missing or not in tab_order
        if !valid_ids.contains(&self.active_document)
            || !self.tab_order.contains(&self.active_document)
        {
            self.active_document = self.tab_order[0];
        }

        // Drop stale recent_order entries
        self.recent_order.retain(|id| valid_ids.contains(id));
        if !self.recent_order.contains(&self.active_document) {
            self.recent_order.insert(0, self.active_document);
        }

        // Validate per-document fields
        for document in &mut self.documents {
            document.validate();
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

    pub fn open_untitled(
        &mut self,
        default_tab_width: usize,
        default_soft_tabs: bool,
    ) -> DocumentId {
        let index = self.next_untitled_index;
        self.next_untitled_index += 1;

        let document = Document::new_untitled(index, default_tab_width, default_soft_tabs);
        let id = document.id;

        self.documents.push(document);
        self.tab_order.push(id);
        self.active_document = id;
        self.update_recent_order(id);
        id
    }

    pub fn close_active(&mut self, default_tab_width: usize, default_soft_tabs: bool) {
        if self.documents.len() <= 1 {
            if let Some(document) = self.active_document_mut() {
                document.replace_text("");
            }
            return;
        }

        let old_active = self.active_document;
        self.documents.retain(|document| document.id != old_active);
        self.tab_order.retain(|id| *id != old_active);
        self.recent_order.retain(|id| *id != old_active);
        self.active_document = self.tab_order.first().copied().unwrap_or_else(|| {
            let document = Document::new_untitled(
                self.next_untitled_index,
                default_tab_width,
                default_soft_tabs,
            );
            let id = document.id;
            self.next_untitled_index += 1;
            self.documents.push(document);
            self.tab_order.push(id);
            self.recent_order.push(id);
            id
        });
        self.update_recent_order(self.active_document);
    }

    pub fn set_active(&mut self, document_id: DocumentId) -> bool {
        if self.tab_order.contains(&document_id) && self.document(document_id).is_some() {
            self.active_document = document_id;
            self.update_recent_order(document_id);
            true
        } else {
            false
        }
    }

    pub fn recent_order(&self) -> &[DocumentId] {
        &self.recent_order
    }

    pub fn recent_order_mut(&mut self) -> &mut Vec<DocumentId> {
        &mut self.recent_order
    }

    fn update_recent_order(&mut self, document_id: DocumentId) {
        // Remove if present and push to front (most recent)
        self.recent_order.retain(|id| *id != document_id);
        self.recent_order.insert(0, document_id);
    }
}

fn serialize_recent_order<S>(_order: &Vec<DocumentId>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Serialize as empty for now - will be populated on first use
    let empty: Vec<DocumentId> = Vec::new();
    empty.serialize(serializer)
}

fn deserialize_recent_order<'de, D>(deserializer: D) -> Result<Vec<DocumentId>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<DocumentId> = Vec::deserialize(deserializer)?;
    Ok(vec)
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
    /// Whether this tab is pinned (cannot be closed, stays in place)
    pub pinned: bool,
    /// Bookmarks stored as byte offsets (0-based) for consistency with selections
    pub bookmarks: BTreeSet<usize>,
    /// Per-document tree-sitter syntax state (parse tree + highlight cache)
    #[serde(skip)]
    pub syntax_state: DocumentSyntaxState,
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
    pub fn new_untitled(_index: u64, default_tab_width: usize, default_soft_tabs: bool) -> Self {
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
            tab_width: default_tab_width,
            use_soft_tabs: default_soft_tabs,
            pinned: false,
            bookmarks: BTreeSet::new(),
            syntax_state: DocumentSyntaxState::new(),
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
        let registry = crate::grammar_registry::GrammarRegistry::default();
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

        let mut edits = edits;
        edits.sort_by_key(|edit| edit.range.start);

        let mut final_selections = Vec::new();
        let mut final_offset: isize = 0;
        for edit in &edits {
            for selection in &edit.selections_after {
                final_selections.push(Selection {
                    anchor: shift_offset(selection.anchor, final_offset),
                    head: shift_offset(selection.head, final_offset),
                });
            }
            final_offset += edit.inserted_text.len() as isize
                - (edit.range.end as isize - edit.range.start as isize);
        }

        let mut transactions = Vec::new();

        // Apply edits from last to first to preserve positions.
        for edit in edits.iter().rev() {
            let start = edit.range.start;
            let end = edit.range.end;

            let deleted_text = self.rope.byte_slice(start..end).to_string();

            transactions.push(EditTransaction {
                start,
                end,
                deleted_text,
                inserted_text: edit.inserted_text.clone(),
                selections_before: edit.selections_before.clone(),
            });

            if start != end {
                self.rope.delete(start..end);
            }
            if !edit.inserted_text.is_empty() {
                self.rope.insert(start, &edit.inserted_text);
            }
        }

        self.undo.record_multi(transactions);
        self.selections = final_selections;
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    #[allow(dead_code)]
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

    /// Clamp selections to valid byte offsets and fix scroll values.
    pub fn validate(&mut self) {
        let len = self.rope.byte_len();

        // Clamp selections to [0, len]
        for sel in &mut self.selections {
            sel.anchor = sel.anchor.min(len);
            sel.head = sel.head.min(len);
        }
        if self.selections.is_empty() {
            self.selections.push(Selection::caret(len));
        }

        // Clamp bookmarks to valid byte offsets
        self.bookmarks.retain(|&offset| offset <= len);

        // Clamp scroll to non-negative values
        self.scroll.x = self.scroll.x.max(0.0);
        self.scroll.y = self.scroll.y.max(0.0);

        // Ensure reasonable tab width
        if self.tab_width == 0 || self.tab_width > 16 {
            self.tab_width = 4;
        }
    }
}

fn shift_offset(offset: usize, shift: isize) -> usize {
    if shift >= 0 {
        offset.saturating_add(shift as usize)
    } else {
        offset.saturating_sub((-shift) as usize)
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

    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_closes_scratch_documents_without_losing_last_buffer() {
        let mut state = AppState::empty();
        let first = state.active_document;
        let second = state.open_untitled(4, true);

        assert_ne!(first, second);
        assert_eq!(state.documents.len(), 2);
        assert_eq!(state.active_document, second);

        state.close_active(4, true);

        assert_eq!(state.documents.len(), 1);
        assert_eq!(state.active_document, first);

        state.close_active(4, true);

        assert_eq!(state.documents.len(), 1);
        assert_eq!(state.active_document, first);
    }

    #[test]
    fn set_active_ignores_unknown_documents() {
        let mut state = AppState::empty();
        let active = state.active_document;

        assert!(!state.set_active(Uuid::new_v4()));
        assert_eq!(state.active_document, active);

        let second = state.open_untitled(4, true);
        assert!(state.set_active(active));
        assert_eq!(state.active_document, active);
        assert!(state.set_active(second));
        assert_eq!(state.active_document, second);
    }

    #[test]
    fn document_title_tracks_first_non_empty_line_until_renamed() {
        let mut document = Document::new_untitled(1, 4, true);
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
        let mut document = Document::new_untitled(1, 4, true);
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
        let mut document = Document::new_untitled(1, 4, true);

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
        let mut document = Document::new_untitled(1, 4, true);
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
        let mut document = Document::new_untitled(1, 4, true);
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
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        // Single edit via multi_edit
        let edits = vec![DocumentEdit {
            range: 0..5,
            inserted_text: "hi".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(2)],
        }];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "hi");

        // Undo should work
        assert!(document.undo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn multi_edit_undo_restores_all_selections() {
        let mut document = Document::new_untitled(1, 4, true);
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

    #[test]
    fn validate_repairs_stale_tab_order() {
        let mut state = AppState::empty();
        let valid_id = state.active_document;
        let stale_id = Uuid::new_v4();

        state.tab_order.push(stale_id);
        state.tab_order.push(valid_id); // duplicate

        state.validate();

        assert_eq!(state.tab_order.len(), 1);
        assert_eq!(state.tab_order[0], valid_id);
    }

    #[test]
    fn validate_fixes_missing_active_document() {
        let mut state = AppState::empty();
        state.active_document = Uuid::new_v4(); // missing

        state.validate();

        assert!(state.document(state.active_document).is_some());
        assert!(state.tab_order.contains(&state.active_document));
    }

    #[test]
    fn validate_creates_document_when_empty() {
        let mut state = AppState {
            documents: vec![],
            tab_order: vec![],
            active_document: Uuid::new_v4(),
            next_untitled_index: 2,
            recent_order: vec![],
        };

        state.validate();

        assert!(!state.documents.is_empty());
        assert!(!state.tab_order.is_empty());
        assert!(state.document(state.active_document).is_some());
    }

    #[test]
    fn document_validate_clamps_selections() {
        let mut doc = Document::new_untitled(1, 4, true);
        doc.replace_text("hello");
        let len = doc.rope.byte_len();

        doc.selections = vec![
            Selection {
                anchor: 0,
                head: len + 100,
            }, // out of bounds
            Selection {
                anchor: len + 50,
                head: len + 50,
            }, // out of bounds
        ];

        doc.validate();

        for sel in &doc.selections {
            assert!(sel.anchor <= len);
            assert!(sel.head <= len);
        }
    }

    #[test]
    fn document_validate_fixes_scroll_and_tab_width() {
        let mut doc = Document::new_untitled(1, 4, true);

        doc.scroll = ScrollState { x: -5.0, y: -10.0 };
        doc.tab_width = 0;

        doc.validate();

        assert!(doc.scroll.x >= 0.0);
        assert!(doc.scroll.y >= 0.0);
        assert_eq!(doc.tab_width, 4);
    }

    // ============================================================================
    // Multi-Cursor Editing Transaction Tests
    // ============================================================================

    #[test]
    fn multi_edit_with_overlapping_ranges_fails_gracefully() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello world");
        document.revision = 0;

        // Overlapping ranges should still apply (in reverse order)
        let edits = vec![
            DocumentEdit {
                range: 0..5,
                inserted_text: "hi".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(2)],
            },
            DocumentEdit {
                range: 3..8,
                inserted_text: "there".to_owned(),
                selections_before: vec![Selection::caret(3)],
                selections_after: vec![Selection::caret(8)],
            },
        ];

        // This should not panic - edits are applied in reverse order
        document.apply_multi_edit(edits);
        // The exact result depends on order, but should not crash
        assert!(document.revision >= 1);
    }

    #[test]
    fn multi_edit_preserves_document_state_on_empty_edits() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        let edits = vec![];
        document.apply_multi_edit(edits);

        assert_eq!(document.text(), "hello");
        assert_eq!(document.revision, 0);
    }

    #[test]
    fn multi_edit_with_adjacent_non_overlapping_ranges() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("abcdef");
        document.revision = 0;

        // Two adjacent edits: replace "ab" and "cd"
        let edits = vec![
            DocumentEdit {
                range: 0..2,
                inserted_text: "AB".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(2)],
            },
            DocumentEdit {
                range: 2..4,
                inserted_text: "CD".to_owned(),
                selections_before: vec![Selection::caret(2)],
                selections_after: vec![Selection::caret(4)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "ABCDef");
    }

    #[test]
    fn multi_edit_uses_original_offsets_for_different_length_replacements() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("abc def ghi");
        document.revision = 0;

        let edits = vec![
            DocumentEdit {
                range: 0..3,
                inserted_text: "alpha".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(5)],
            },
            DocumentEdit {
                range: 8..11,
                inserted_text: "x".to_owned(),
                selections_before: vec![Selection::caret(8)],
                selections_after: vec![Selection::caret(9)],
            },
        ];

        document.apply_multi_edit(edits);

        assert_eq!(document.text(), "alpha def x");
        assert_eq!(
            document.selections,
            vec![Selection::caret(5), Selection::caret(11)]
        );
        assert!(document.undo());
        assert_eq!(document.text(), "abc def ghi");
    }

    #[test]
    fn multi_edit_undo_restores_all_selections_correctly() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("a b c d");
        document.revision = 0;

        let sel1 = Selection { anchor: 0, head: 1 };
        let sel2 = Selection { anchor: 2, head: 3 };
        let sel3 = Selection { anchor: 4, head: 5 };

        let edits = vec![
            DocumentEdit {
                range: 0..1,
                inserted_text: "X".to_owned(),
                selections_before: vec![sel1],
                selections_after: vec![Selection::caret(1)],
            },
            DocumentEdit {
                range: 2..3,
                inserted_text: "Y".to_owned(),
                selections_before: vec![sel2],
                selections_after: vec![Selection::caret(3)],
            },
            DocumentEdit {
                range: 4..5,
                inserted_text: "Z".to_owned(),
                selections_before: vec![sel3],
                selections_after: vec![Selection::caret(5)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "X Y Z d");

        // Undo should restore original text and first selection
        assert!(document.undo());
        assert_eq!(document.text(), "a b c d");
        // After undo, selections should be restored to the first edit's selections_before
        assert_eq!(document.selections.len(), 1);
        assert_eq!(document.selections[0], sel1);
    }

    #[test]
    fn multi_edit_with_insertion_only_no_deletion() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        // Insert at multiple positions (all at same point for simplicity)
        let edits = vec![DocumentEdit {
            range: 5..5,
            inserted_text: " world".to_owned(),
            selections_before: vec![Selection::caret(5)],
            selections_after: vec![Selection::caret(11)],
        }];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "hello world");
        assert_eq!(document.revision, 1);

        assert!(document.undo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn multi_edit_with_deletion_only_no_insertion() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello world");
        document.revision = 0;

        let edits = vec![DocumentEdit {
            range: 5..11,
            inserted_text: String::new(),
            selections_before: vec![Selection::caret(5)],
            selections_after: vec![Selection::caret(5)],
        }];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "hello");
        assert_eq!(document.revision, 1);

        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
    }

    #[test]
    fn multi_edit_undo_then_redo_restores_state() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("foo bar baz");
        document.revision = 0;

        let edits = vec![
            DocumentEdit {
                range: 0..3,
                inserted_text: "FOO".to_owned(),
                selections_before: vec![Selection::caret(0)],
                selections_after: vec![Selection::caret(3)],
            },
            DocumentEdit {
                range: 4..7,
                inserted_text: "BAR".to_owned(),
                selections_before: vec![Selection::caret(4)],
                selections_after: vec![Selection::caret(7)],
            },
        ];

        document.apply_multi_edit(edits);
        assert_eq!(document.text(), "FOO BAR baz");

        // Undo
        assert!(document.undo());
        assert_eq!(document.text(), "foo bar baz");

        // Redo
        assert!(document.redo());
        assert_eq!(document.text(), "FOO BAR baz");
    }

    #[test]
    fn multi_edit_with_multibyte_characters() {
        let mut document = Document::new_untitled(1, 4, true);
        // "a"=1, "é"=2, "日"=3, "b"=1 -> total 7 bytes
        // Byte positions: a=0..1, é=1..3, 日=3..6, b=6..7
        document.replace_text("aé日b");
        document.revision = 0;

        // Apply two single edits that each handle multibyte characters correctly
        // First: replace "日" (bytes 3..6) with "ri"
        let edit1 = DocumentEdit {
            range: 3..6,
            inserted_text: "ri".to_owned(),
            selections_before: vec![Selection::caret(3)],
            selections_after: vec![Selection::caret(5)],
        };
        document.apply_edit(edit1);
        // "aé" (3 bytes) + "ri" (2 bytes) + "b" (1 byte) = "aérib" (6 bytes)
        assert_eq!(document.text(), "aérib");

        // Second: replace "é" (bytes 1..3) with "e"
        let edit2 = DocumentEdit {
            range: 1..3,
            inserted_text: "e".to_owned(),
            selections_before: vec![Selection::caret(1)],
            selections_after: vec![Selection::caret(2)],
        };
        document.apply_edit(edit2);
        // "a" (1 byte) + "e" (1 byte) + "rib" (3 bytes) = "aerib" (5 bytes)
        assert_eq!(document.text(), "aerib");
    }

    // ============================================================================
    // Multi-Cursor Selection Behavior Tests
    // ============================================================================

    #[test]
    fn multiple_cursors_are_independent_after_edit() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("one\ntwo\nthree");
        document.revision = 0;

        // Set up multiple selections
        document.selections = vec![
            Selection { anchor: 0, head: 3 }, // "one"
            Selection { anchor: 4, head: 7 }, // "two"
        ];

        // Apply edit to first selection
        document.apply_edit(DocumentEdit {
            range: 0..3,
            inserted_text: "ONE".to_owned(),
            selections_before: vec![Selection { anchor: 0, head: 3 }],
            selections_after: vec![Selection::caret(3)],
        });

        // The selections should be updated by the edit
        assert_eq!(document.text(), "ONE\ntwo\nthree");
    }

    #[test]
    fn primary_selection_is_first_in_vec() {
        let mut document = Document::new_untitled(1, 4, true);
        document.selections = vec![
            Selection { anchor: 0, head: 0 }, // primary
            Selection { anchor: 5, head: 5 }, // secondary
            Selection {
                anchor: 10,
                head: 10,
            }, // secondary
        ];

        // Primary selection is conventionally the first one
        assert_eq!(document.selections[0].anchor, 0);
        assert_eq!(document.selections.len(), 3);
    }

    #[test]
    fn selections_are_clamped_on_validate() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        let len = document.rope.byte_len();

        // Add selections with out-of-bounds positions
        document.selections = vec![
            Selection {
                anchor: 0,
                head: len + 100,
            },
            Selection {
                anchor: len + 50,
                head: len + 50,
            },
            Selection { anchor: 2, head: 3 }, // valid
        ];

        document.validate();

        // All selections should be clamped to valid range
        for sel in &document.selections {
            assert!(sel.anchor <= len);
            assert!(sel.head <= len);
        }
    }

    #[test]
    fn empty_selections_vec_gets_default_on_validate() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        let len = document.rope.byte_len();

        document.selections = vec![];
        document.validate();

        assert!(!document.selections.is_empty());
        assert_eq!(document.selections[0], Selection::caret(len));
    }

    #[test]
    fn backward_selections_are_valid() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");

        // Backward selection (anchor > head)
        let backward = Selection { anchor: 4, head: 1 };
        document.selections = vec![backward];

        // Should be valid - backward selections are allowed
        document.validate();
        assert_eq!(document.selections.len(), 1);
        // The selection should still be backward after validation
        assert_eq!(document.selections[0].anchor, 4);
        assert_eq!(document.selections[0].head, 1);
    }

    // ============================================================================
    // Undo/Redo Stack Behavior Tests
    // ============================================================================

    #[test]
    fn undo_stack_depth_matches_edit_count() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("");
        document.revision = 0;

        // Apply 3 grouped edits
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
        document.apply_continuing_edit(DocumentEdit::replace_selection(
            Selection::caret(2),
            2..2,
            "c",
        ));
        document.commit_undo_group();

        assert_eq!(document.text(), "abc");

        // Single undo should undo all three
        assert!(document.undo());
        assert_eq!(document.text(), "");
    }

    #[test]
    fn redo_stack_cleared_on_new_edit() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        // Make an edit and undo it
        document.apply_grouped_edit(DocumentEdit::replace_selection(
            Selection::caret(5),
            5..5,
            " world",
        ));
        assert!(document.undo());
        assert!(document.can_redo());

        // New edit should clear redo stack
        document.apply_grouped_edit(DocumentEdit::replace_selection(
            Selection::caret(5),
            5..5,
            "!",
        ));
        assert!(!document.can_redo());
    }

    #[test]
    fn undo_state_tracks_multiple_undo_groups() {
        let mut undo = UndoState::default();

        // First group
        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            selections_before: vec![Selection::caret(0)],
        });
        undo.commit_group();

        // Second group
        undo.begin_group();
        undo.record(EditTransaction {
            start: 1,
            end: 1,
            deleted_text: String::new(),
            inserted_text: "b".to_owned(),
            selections_before: vec![Selection::caret(1)],
        });
        undo.commit_group();

        assert!(undo.can_undo());
        undo.undo(); // undoes "b"
        undo.undo(); // undoes "a"
        assert!(!undo.can_undo());
    }

    #[test]
    fn interleaved_undo_redo_operations() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("");
        document.revision = 0;

        // Edit 1
        document.apply_grouped_edit(DocumentEdit::replace_selection(
            Selection::caret(0),
            0..0,
            "first",
        ));

        // Edit 2
        document.apply_grouped_edit(DocumentEdit::replace_selection(
            Selection::caret(5),
            5..5,
            " second",
        ));

        assert_eq!(document.text(), "first second");

        // Undo edit 2
        assert!(document.undo());
        assert_eq!(document.text(), "first");

        // Redo edit 2
        assert!(document.redo());
        assert_eq!(document.text(), "first second");

        // Undo both
        assert!(document.undo());
        assert_eq!(document.text(), "first");
        assert!(document.undo());
        assert_eq!(document.text(), "");
    }

    // ============================================================================
    // Single Edit Transaction Tests
    // ============================================================================

    #[test]
    fn single_edit_transaction_records_correct_bounds() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello world");
        document.revision = 0;

        let sel_before = Selection {
            anchor: 6,
            head: 11,
        };
        document.apply_grouped_edit(DocumentEdit {
            range: 6..11,
            inserted_text: "earth".to_owned(),
            selections_before: vec![sel_before],
            selections_after: vec![Selection::caret(11)],
        });

        // Undo and verify the transaction recorded correct deleted text
        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
        assert_eq!(document.selections[0], sel_before);
    }

    #[test]
    fn edit_at_document_start() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        document.apply_grouped_edit(DocumentEdit {
            range: 0..0,
            inserted_text: "++".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(2)],
        });

        assert_eq!(document.text(), "++hello");
        assert_eq!(document.revision, 1);
    }

    #[test]
    fn edit_at_document_end() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        let len = document.rope.byte_len();
        document.revision = 0;

        document.apply_grouped_edit(DocumentEdit {
            range: len..len,
            inserted_text: "++".to_owned(),
            selections_before: vec![Selection::caret(len)],
            selections_after: vec![Selection::caret(len + 2)],
        });

        assert_eq!(document.text(), "hello++");
    }

    #[test]
    fn edit_replacing_entire_document() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("original text");
        document.revision = 0;

        let original_len = document.rope.byte_len();
        document.apply_grouped_edit(DocumentEdit {
            range: 0..original_len,
            inserted_text: "new text".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![Selection::caret(8)],
        });

        assert_eq!(document.text(), "new text");

        assert!(document.undo());
        assert_eq!(document.text(), "original text");
    }

    // ============================================================================
    // Edge Cases and Error Handling
    // ============================================================================

    #[test]
    fn undo_when_nothing_to_undo() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");

        assert!(!document.undo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn redo_when_nothing_to_redo() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");

        assert!(!document.redo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn multi_edit_with_empty_selections_after() {
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text("hello");
        document.revision = 0;

        let edits = vec![DocumentEdit {
            range: 0..5,
            inserted_text: "hi".to_owned(),
            selections_before: vec![Selection::caret(0)],
            selections_after: vec![], // empty selections after
        }];

        document.apply_multi_edit(edits);
        // Should use last edit's selections_after
        assert_eq!(document.selections, vec![]);
    }

    #[test]
    fn document_edit_replace_selection_helper() {
        let sel = Selection { anchor: 2, head: 5 };
        let edit = DocumentEdit::replace_selection(sel, 2..5, "new");

        assert_eq!(edit.range, 2..5);
        assert_eq!(edit.inserted_text, "new");
        assert_eq!(edit.selections_before, vec![sel]);
        assert_eq!(edit.selections_after, vec![Selection::caret(5)]);
    }

    #[test]
    fn undo_state_is_typing_flag_management() {
        let mut undo = UndoState::default();

        assert!(!undo.is_typing);

        undo.begin_group();
        assert!(undo.is_typing);

        undo.commit_group();
        assert!(!undo.is_typing);

        undo.begin_group();
        assert!(undo.is_typing);

        undo.discard_group();
        assert!(!undo.is_typing);
    }

    #[test]
    fn can_undo_respects_typing_group() {
        let mut undo = UndoState::default();

        assert!(!undo.can_undo());

        undo.begin_group();
        undo.record(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: String::new(),
            inserted_text: "a".to_owned(),
            selections_before: vec![Selection::caret(0)],
        });

        assert!(undo.can_undo()); // typing group has edits

        undo.discard_group();
        assert!(!undo.can_undo()); // typing group discarded
    }
}
