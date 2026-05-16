use std::collections::BTreeSet;

use crop::{Rope, RopeSlice};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::syntax::{LanguageDetection, LanguageId};
use crate::syntax_highlighting::DocumentSyntaxState;

use super::{DocumentId, EditTransaction, ScrollState, Selection, UndoState};

const FALLBACK_TITLE: &str = "Untitled";
const MAX_AUTO_TITLE_CHARS: usize = 48;

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
    /// Explicit syntax selection for this scratch document. `None` uses auto detection.
    #[serde(default)]
    pub syntax_override: Option<LanguageId>,
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
            syntax_override: None,
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
        if let Some(language) = self.syntax_override {
            return Some(LanguageDetection {
                language,
                confidence: 1.0,
            });
        }

        Some(crate::grammar_registry::GrammarRegistry::shared().detect_rope(&self.rope))
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

        let edits = normalize_multi_edits(edits, &self.rope);
        if edits.is_empty() {
            return;
        }

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
        self.validate();
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

fn normalize_multi_edits(mut edits: Vec<DocumentEdit>, rope: &Rope) -> Vec<DocumentEdit> {
    let len = rope.byte_len();
    edits.sort_by_key(|edit| edit.range.start);

    let mut normalized = Vec::with_capacity(edits.len());
    let mut covered_until = 0;

    for mut edit in edits {
        let mut start = clamp_offset_to_char_boundary(rope, edit.range.start.min(len));
        let end = clamp_offset_to_char_boundary(rope, edit.range.end.min(len));

        if end < start {
            continue;
        }

        if start < covered_until {
            if end <= covered_until {
                continue;
            }
            start = covered_until;
        }

        if start == end && edit.inserted_text.is_empty() {
            continue;
        }

        edit.range = start..end;
        covered_until = covered_until.max(edit.range.end);
        normalized.push(edit);
    }

    normalized
}

fn clamp_offset_to_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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
