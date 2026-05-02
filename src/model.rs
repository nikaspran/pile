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
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn replace_text(&mut self, text: &str) {
        self.rope = Rope::from(text);
        self.revision += 1;
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
}
