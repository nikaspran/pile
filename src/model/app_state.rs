use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Document, DocumentId, Selection};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClosedDocument {
    pub document: Document,
    pub order: u64,
}

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
    pub(crate) recent_order: Vec<DocumentId>,
    #[serde(default)]
    pub closed_documents: Vec<ClosedDocument>,
    #[serde(default)]
    pub next_closed_order: u64,
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
            closed_documents: Vec::new(),
            next_closed_order: 0,
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

        // Validate closed documents
        self.closed_documents
            .retain(|cd| !valid_ids.contains(&cd.document.id));
        for cd in &mut self.closed_documents {
            cd.document.validate();
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
            // Create a new untitled document first, then close the old one
            let document = Document::new_untitled(
                self.next_untitled_index,
                default_tab_width,
                default_soft_tabs,
            );
            let new_id = document.id;
            self.next_untitled_index += 1;
            self.documents.push(document);
            self.tab_order.push(new_id);
            self.recent_order.push(new_id);
        }

        let old_active = self.active_document;
        self.push_closed_document(old_active);
        self.tab_order.retain(|id| *id != old_active);
        self.recent_order.retain(|id| *id != old_active);
        self.active_document = self
            .recent_order
            .iter()
            .copied()
            .find(|id| self.tab_order.contains(id) && self.document(*id).is_some())
            .or_else(|| self.tab_order.first().copied())
            .unwrap_or_else(|| {
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

    pub fn close_document_by_id(&mut self, document_id: DocumentId) -> bool {
        if self.document(document_id).is_none() {
            return false;
        }
        self.push_closed_document(document_id);
        self.tab_order.retain(|id| *id != document_id);
        self.recent_order.retain(|id| *id != document_id);
        true
    }

    fn push_closed_document(&mut self, document_id: DocumentId) {
        if let Some(pos) = self.documents.iter().position(|d| d.id == document_id) {
            let doc = self.documents.remove(pos);
            self.closed_documents.push(ClosedDocument {
                document: doc,
                order: self.next_closed_order,
            });
            self.next_closed_order += 1;
        }
    }

    pub fn reopen_document(&mut self, document_id: DocumentId) -> bool {
        let pos = match self
            .closed_documents
            .iter()
            .position(|cd| cd.document.id == document_id)
        {
            Some(p) => p,
            None => return false,
        };
        let closed = self.closed_documents.remove(pos);
        let mut doc = closed.document;
        doc.selections = vec![Selection::caret(0)];
        let id = doc.id;
        self.documents.push(doc);
        self.tab_order.push(id);
        self.active_document = id;
        self.update_recent_order(id);
        true
    }

    pub fn permanently_delete_document(&mut self, document_id: DocumentId) -> bool {
        let len = self.closed_documents.len();
        self.closed_documents
            .retain(|cd| cd.document.id != document_id);
        self.closed_documents.len() < len
    }

    pub fn last_closed_document(&self) -> Option<&ClosedDocument> {
        self.closed_documents.iter().max_by_key(|cd| cd.order)
    }

    pub fn closed_documents(&self) -> &[ClosedDocument] {
        &self.closed_documents
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

    pub fn move_tab_to_index(&mut self, document_id: DocumentId, target_index: usize) -> bool {
        let Some(current_index) = self.tab_order.iter().position(|id| *id == document_id) else {
            return false;
        };

        let target_index = target_index.min(self.tab_order.len().saturating_sub(1));
        if current_index == target_index {
            return false;
        }

        let document_id = self.tab_order.remove(current_index);
        self.tab_order.insert(target_index, document_id);
        true
    }

    pub fn recent_order(&self) -> &[DocumentId] {
        &self.recent_order
    }

    #[allow(dead_code)]
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

pub(crate) fn deserialize_recent_order<'de, D>(deserializer: D) -> Result<Vec<DocumentId>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<DocumentId> = Vec::deserialize(deserializer)?;
    Ok(vec)
}
