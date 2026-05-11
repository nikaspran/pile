use super::Selection;

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
