//! UI smoke tests for tab switching, renaming, shortcuts, and session restore.
//!
//! These tests verify that core UI operations work correctly by testing
//! the underlying logic that the UI calls.

use crop::Rope;
use pile::model::AppState;

#[test]
fn smoke_tab_creation() {
    let mut state = AppState::empty();

    // Verify we start with one document
    assert_eq!(state.documents.len(), 1);

    // Open a new tab
    let new_id = state.open_untitled(4, true);
    assert_eq!(state.documents.len(), 2);
    assert_eq!(state.active_document, new_id);
}

#[test]
fn smoke_tab_rename() {
    let mut state = AppState::empty();

    let doc_id = state.active_document;

    // Rename the document
    {
        let doc = state.document_mut(doc_id).unwrap();
        doc.rename("My Notes");
    }

    // Verify rename
    let doc = state.document(doc_id).unwrap();
    assert_eq!(doc.display_title(), "My Notes");
    assert!(doc.has_manual_title());
}

#[test]
fn smoke_tab_close() {
    let mut state = AppState::empty();

    // Open a second tab
    state.open_untitled(4, true);
    assert_eq!(state.documents.len(), 2);

    // Close active tab
    state.close_active(4, true);
    assert_eq!(state.documents.len(), 1);
}

#[test]
fn smoke_tab_switching() {
    let mut state = AppState::empty();

    // Open more tabs
    let id1 = state.open_untitled(4, true);
    let id2 = state.open_untitled(4, true);

    assert_eq!(state.active_document, id2);

    // Switch to first tab
    assert!(state.set_active(id1));
    assert_eq!(state.active_document, id1);
}

#[test]
fn smoke_tab_order() {
    let mut state = AppState::empty();

    // Open tabs
    state.open_untitled(4, true);
    state.open_untitled(4, true);

    // Verify tab order
    assert!(state.tab_order.len() >= 3);
    assert!(state.tab_order.contains(&state.active_document));
}

#[test]
fn smoke_search_integration() {
    let mut state = AppState::empty();

    // Add content to active document
    {
        let doc_id = state.active_document;
        let doc = state.document_mut(doc_id).unwrap();
        doc.rope = Rope::from("hello test world test");
    }

    // Perform search
    let rope = {
        let doc = state.active_document().unwrap();
        doc.rope.clone()
    };

    let matches = pile::search::find_matches(
        &rope,
        "test",
        pile::search::SearchOptions {
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
        },
    );

    assert_eq!(matches.len(), 2);
}

#[test]
fn smoke_document_rename_flow() {
    let mut state = AppState::empty();

    let doc_id = state.active_document;

    // Rename flow
    {
        let doc = state.document_mut(doc_id).unwrap();
        doc.rename("Meeting Notes");
    }

    // Verify
    let doc = state.document(doc_id).unwrap();
    assert_eq!(doc.display_title(), "Meeting Notes");
}

#[test]
fn smoke_settings_defaults() {
    let settings = pile::settings::Settings::default();

    // Verify default settings
    assert!(settings.default_tab_width > 0);
}

#[test]
fn smoke_command_enum_exists() {
    // Verify command enum variants exist
    use pile::command::Command;

    // Just create and match on some commands
    let _cmd = Command::NewScratch;
    let _cmd2 = Command::Undo;
    let _cmd3 = Command::Find;
}

#[test]
fn smoke_recent_order() {
    let mut state = AppState::empty();
    state.open_untitled(4, true);
    state.open_untitled(4, true);

    let recent = state.recent_order();
    assert!(recent.len() >= 3);
}

#[test]
fn smoke_active_document_access() {
    let mut state = AppState::empty();

    // Verify active document access
    let active = state.active_document();
    assert!(active.is_some());

    let active_mut = state.active_document_mut();
    assert!(active_mut.is_some());
}

#[test]
fn smoke_document_title_display() {
    let mut state = AppState::empty();

    let doc_id = state.active_document;

    // Check default title
    {
        let doc = state.document(doc_id).unwrap();
        let title = doc.display_title();
        assert!(!title.is_empty());
    }

    // Rename and check
    {
        let doc = state.document_mut(doc_id).unwrap();
        doc.rename("Test Doc");
    }

    let doc = state.document(doc_id).unwrap();
    assert_eq!(doc.display_title(), "Test Doc");
}

#[test]
fn smoke_session_save_and_restore() {
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let session_path = dir.path().join(".session.bin");

    // Create state and add some documents
    let mut state = AppState::empty();
    state.open_untitled(4, true);
    state.open_untitled(4, true);

    let doc_count_before = state.documents.len();
    let active_before = state.active_document;

    // Save session using SessionSnapshot
    let snapshot: pile::model::SessionSnapshot = (&state).into();
    let snapshot_bytes = serde_json::to_vec(&snapshot).unwrap();

    // Write to file (simulating what SaveWorker does)
    std::fs::write(&session_path, &snapshot_bytes).unwrap();

    // Load session
    let loaded_bytes = std::fs::read(&session_path).unwrap();
    let loaded: Result<pile::model::SessionSnapshot, _> = serde_json::from_slice(&loaded_bytes);
    assert!(loaded.is_ok());

    let snapshot = loaded.unwrap();
    let mut new_state = snapshot.state;
    new_state.validate();

    // Verify restored state
    assert_eq!(new_state.documents.len(), doc_count_before);
    assert!(!new_state.tab_order.is_empty());
    assert_eq!(new_state.active_document, active_before);
}
