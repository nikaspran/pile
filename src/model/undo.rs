use serde::{Deserialize, Serialize};

use super::Selection;

pub const MAX_UNDO_GROUPS: usize = 10;
pub const MAX_PERSISTED_UNDO_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditTransaction {
    pub start: usize,
    pub end: usize,
    pub deleted_text: String,
    pub inserted_text: String,
    pub selections_before: Vec<Selection>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct PersistedUndoStacks {
    pub undo_stack: Vec<Vec<EditTransaction>>,
    pub redo_stack: Vec<Vec<EditTransaction>>,
}

impl PersistedUndoStacks {
    pub fn is_empty(&self) -> bool {
        self.undo_stack.is_empty() && self.redo_stack.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct UndoState {
    pub(crate) undo_stack: Vec<Vec<EditTransaction>>,
    pub(crate) redo_stack: Vec<Vec<EditTransaction>>,
    pub(crate) typing_group: Vec<EditTransaction>,
    pub(crate) is_typing: bool,
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
                self.trim_undo_stack();
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
            self.trim_undo_stack();
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
            self.trim_undo_stack();
        }
    }

    pub fn commit_group(&mut self) {
        if self.is_typing {
            self.is_typing = false;
            if !self.typing_group.is_empty() {
                self.undo_stack.push(std::mem::take(&mut self.typing_group));
                self.redo_stack.clear();
                self.trim_undo_stack();
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

    pub fn export_persisted(&mut self) -> PersistedUndoStacks {
        self.commit_group();
        self.trim_undo_stack();

        let mut undo_stack = self.undo_stack.clone();
        trim_stacks_to_byte_budget(&mut undo_stack, MAX_PERSISTED_UNDO_BYTES);

        PersistedUndoStacks {
            undo_stack,
            redo_stack: self.redo_stack.clone(),
        }
    }

    pub fn import_persisted(
        &mut self,
        persisted: PersistedUndoStacks,
        document_len: usize,
    ) -> bool {
        if !history_is_valid(&persisted, document_len) {
            return false;
        }
        self.clear();
        self.undo_stack = persisted.undo_stack;
        self.redo_stack = persisted.redo_stack;
        true
    }

    fn trim_undo_stack(&mut self) {
        while self.undo_stack.len() > MAX_UNDO_GROUPS {
            self.undo_stack.remove(0);
        }
    }
}

fn group_byte_size(group: &[EditTransaction]) -> usize {
    group
        .iter()
        .map(|txn| txn.deleted_text.len() + txn.inserted_text.len())
        .sum()
}

fn stacks_byte_size(undo_stack: &[Vec<EditTransaction>]) -> usize {
    undo_stack.iter().map(|group| group_byte_size(group)).sum()
}

fn trim_stacks_to_byte_budget(undo_stack: &mut Vec<Vec<EditTransaction>>, max_bytes: usize) {
    while stacks_byte_size(undo_stack) > max_bytes && !undo_stack.is_empty() {
        undo_stack.remove(0);
    }
}

fn history_is_valid(persisted: &PersistedUndoStacks, document_len: usize) -> bool {
    for group in &persisted.undo_stack {
        for txn in group {
            if txn.start > document_len
                || txn.start > txn.end
                || txn.start.saturating_add(txn.inserted_text.len()) > document_len
            {
                return false;
            }
        }
    }
    for group in &persisted.redo_stack {
        for txn in group {
            if txn.start > document_len
                || txn.start.saturating_add(txn.deleted_text.len()) > document_len
            {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_txn(start: usize, end: usize, deleted: &str, inserted: &str) -> EditTransaction {
        EditTransaction {
            start,
            end,
            deleted_text: deleted.to_owned(),
            inserted_text: inserted.to_owned(),
            selections_before: vec![Selection::caret(start)],
        }
    }

    #[test]
    fn undo_stack_trims_to_max_groups() {
        let mut undo = UndoState::default();
        for i in 0..12 {
            undo.record(sample_txn(i, i, "", &format!("{i}")));
        }
        assert_eq!(undo.undo_stack.len(), MAX_UNDO_GROUPS);
        assert_eq!(undo.undo_stack[0][0].inserted_text, "2");
        assert_eq!(undo.undo_stack.last().unwrap()[0].inserted_text, "11");
    }

    #[test]
    fn export_applies_byte_budget() {
        let mut undo = UndoState::default();
        undo.undo_stack
            .push(vec![sample_txn(0, 0, "", &"x".repeat(300_000))]);
        undo.undo_stack
            .push(vec![sample_txn(0, 0, "", &"y".repeat(300_000))]);

        let exported = undo.export_persisted();
        assert_eq!(exported.undo_stack.len(), 1);
        assert_eq!(exported.undo_stack[0][0].inserted_text, "y".repeat(300_000));
    }

    #[test]
    fn import_rejects_invalid_offsets() {
        let mut undo = UndoState::default();
        let persisted = PersistedUndoStacks {
            undo_stack: vec![vec![sample_txn(100, 100, "", "x")]],
            redo_stack: Vec::new(),
        };
        assert!(!undo.import_persisted(persisted, 10));
        assert!(!undo.can_undo());
    }
}
