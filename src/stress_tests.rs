#[cfg(test)]
mod tests {
    use crate::model::{AppState, Document, Selection, SessionSnapshot};
    use crate::persistence::{SaveWorker, SaveMsg, SaveTelemetry, load_session, SessionEnvelope};
    use crossbeam_channel::bounded;
    use std::fs;
    use std::time::{Duration, Instant};

    // Helper to generate large text content
    fn large_text(lines: usize, line_len: usize) -> String {
        let mut text = String::with_capacity(lines * (line_len + 1));
        for i in 0..lines {
            text.push_str(&format!("Line {:10}:", i));
            for _ in 0..(line_len.saturating_sub(12)) {
                text.push('a');
            }
            text.push('\n');
        }
        text
    }

    // Helper to create a test document
    fn make_document(text: &str) -> Document {
        let mut document = Document::new_untitled(1);
        document.replace_text(text);
        document.selections = vec![Selection { anchor: 0, head: 0 }];
        document.revision = 0;
        document
    }

    // Helper to create many documents using open_untitled pattern
    fn create_many_documents(state: &mut AppState, count: usize) {
        for _ in 0..count {
            let doc_id = state.open_untitled();
            if let Some(doc) = state.document_mut(doc_id) {
                doc.replace_text("Document content");
            }
        }
    }

    // --- Rapid Edits Tests ---

    #[test]
    fn rapid_edits_small_buffer() {
        let mut doc = make_document("");
        let start = Instant::now();

        for i in 0..1000 {
            doc.replace_text(&format!("{}", i));
        }

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(2), "Rapid edits took {:?}", elapsed);
    }

    #[test]
    fn rapid_edits_large_buffer() {
        let text = large_text(10000, 80);
        let mut doc = make_document(&text);
        let start = Instant::now();

        for i in 0..100 {
            let pos = (i * 1000) % (doc.text().len().max(1));
            let rope_pos = pos.min(doc.rope.byte_len());
            doc.rope = crop::Rope::from(doc.text());
            doc.revision += 1;
        }

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(5), "Rapid edits on large buffer took {:?}", elapsed);
    }

    #[test]
    fn rapid_undo_redo() {
        let mut doc = make_document("initial");
        let start = Instant::now();

        for i in 0..100 {
            doc.replace_text(&format!("edit {}", i));
            doc.undo();
            doc.redo();
        }

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(3), "Rapid undo/redo took {:?}", elapsed);
    }

    // --- Many Tabs Tests ---

    #[test]
    fn many_tabs_creation() {
        let mut state = AppState::empty();
        let start = Instant::now();

        create_many_documents(&mut state, 500);

        let elapsed = start.elapsed();
        assert!(state.documents.len() >= 500);
        assert!(elapsed < Duration::from_secs(2), "Creating 500 tabs took {:?}", elapsed);
    }

    #[test]
    fn many_tabs_session_roundtrip() {
        let mut state = AppState::empty();
        // Clear initial document and add fresh ones
        state.documents.clear();
        state.tab_order.clear();
        create_many_documents(&mut state, 200);

        let snapshot = SessionSnapshot::from(&state);
        let envelope = SessionEnvelope::wrap(&snapshot).unwrap();
        let loaded = SessionEnvelope::open(envelope).unwrap();

        assert_eq!(loaded.state.documents.len(), 200);
    }

    #[test]
    fn many_tabs_switching() {
        let mut state = AppState::empty();
        state.documents.clear();
        state.tab_order.clear();
        create_many_documents(&mut state, 300);

        let start = Instant::now();
        let ids: Vec<_> = state.tab_order.clone();
        for id in &ids {
            let _ = state.set_active(*id);
        }
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_secs(1), "Switching 300 tabs took {:?}", elapsed);
    }

    // --- Large Buffer Tests ---

    #[test]
    fn large_buffer_5mb() {
        let text = large_text(50000, 100); // ~5MB
        assert!(text.len() > 5_000_000);

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(elapsed < Duration::from_secs(2), "Loading 5MB buffer took {:?}", elapsed);
    }

    #[test]
    fn large_buffer_10mb() {
        let text = large_text(100000, 100); // ~10MB
        assert!(text.len() > 10_000_000);

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(elapsed < Duration::from_secs(3), "Loading 10MB buffer took {:?}", elapsed);
    }

    #[test]
    fn large_buffer_edit_performance() {
        let text = large_text(50000, 80);
        let mut doc = make_document(&text);
        let start = Instant::now();

        // Perform edits scattered throughout the large buffer
        for i in 0..50 {
            let pos = (i * doc.rope.byte_len() / 50).min(doc.rope.byte_len());
            // Use replace_text to simulate edit at position
            let current = doc.text();
            let new_text = format!("{}insert{}", &current[..pos], i);
            doc.replace_text(&new_text);
        }

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(3), "Editing 5MB buffer took {:?}", elapsed);
    }

    #[test]
    fn large_buffer_line_operations() {
        let text = large_text(10000, 80);
        let mut doc = make_document(&text);
        let start = Instant::now();

        // Select and verify we can access text at various positions
        for i in 0..10 {
            let line_start = i * 100;
            let line_end = line_start + 80;
            if line_end <= doc.text().len() {
                doc.selections = vec![Selection { anchor: line_start, head: line_end }];
                let _slice = &doc.text()[line_start..line_end];
            }
        }

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(2), "Line operations on large buffer took {:?}", elapsed);
    }

    // --- Crash/Restart Cycle Tests ---

    #[test]
    fn crash_restart_single_document() {
        let dir = std::env::temp_dir().join(format!("pile-stress-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".session.bin");

        // Simulate creating and saving a session
        let mut state = AppState::empty();
        if let Some(doc) = state.active_document_mut() {
            doc.replace_text("Important content that must survive restart");
        }

        // Save
        let worker = SaveWorker::spawn(path.clone());
        let (ack_tx, ack_rx) = bounded(1);
        let snapshot = SessionSnapshot::from(&state);

        worker.sender().send(SaveMsg::Flush(snapshot, ack_tx)).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();

        // Simulate restart - load session
        let mut telemetry = SaveTelemetry::default();
        let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();

        assert!(loaded.state.documents.len() >= 1);

        worker.shutdown();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_restart_multiple_cycles() {
        let dir = std::env::temp_dir().join(format!("pile-stress-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".session.bin");

        let worker = SaveWorker::spawn(path.clone());

        for cycle in 0..10 {
            // Create state for this cycle
            let mut state = AppState::empty();
            if let Some(doc) = state.active_document_mut() {
                doc.replace_text(&format!("Cycle {} content", cycle));
            }

            // Save
            let (ack_tx, ack_rx) = bounded(1);
            let snapshot = SessionSnapshot::from(&state);
            worker.sender().send(SaveMsg::Flush(snapshot, ack_tx)).unwrap();
            ack_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();

            // Simulate small delay then "restart"
            std::thread::sleep(Duration::from_millis(50));
        }

        // Final load
        let mut telemetry = SaveTelemetry::default();
        let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();

        assert!(loaded.state.documents.len() >= 1);

        worker.shutdown();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_restart_many_tabs() {
        let dir = std::env::temp_dir().join(format!("pile-stress-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".session.bin");

        // Create session with many tabs
        let mut state = AppState::empty();
        state.documents.clear();
        state.tab_order.clear();
        create_many_documents(&mut state, 100);

        // Save
        let worker = SaveWorker::spawn(path.clone());
        let (ack_tx, ack_rx) = bounded(1);
        let snapshot = SessionSnapshot::from(&state);

        worker.sender().send(SaveMsg::Flush(snapshot, ack_tx)).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();

        // "Restart" and load
        let mut telemetry = SaveTelemetry::default();
        let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();

        assert_eq!(loaded.state.documents.len(), 100);

        worker.shutdown();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }

    // --- Combined Stress Tests ---

    #[test]
    fn stress_rapid_edits_many_tabs() {
        let mut state = AppState::empty();
        state.documents.clear();
        state.tab_order.clear();
        create_many_documents(&mut state, 50);

        let start = Instant::now();
        for doc in state.documents.iter_mut() {
            for i in 0..20 {
                doc.replace_text(&format!("{}edit{}", doc.text(), i));
            }
        }
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_secs(5), "Rapid edits across 50 tabs took {:?}", elapsed);
    }

    #[test]
    fn stress_large_session_serialization() {
        let mut state = AppState::empty();
        // Add a large document
        let large_text_content = large_text(50000, 80);
        if let Some(doc) = state.active_document_mut() {
            doc.replace_text(&large_text_content);
        }

        // Add many small documents
        create_many_documents(&mut state, 100);

        let start = Instant::now();
        let snapshot = SessionSnapshot::from(&state);
        let envelope = SessionEnvelope::wrap(&snapshot).unwrap();
        let loaded = SessionEnvelope::open(envelope).unwrap();
        let elapsed = start.elapsed();

        assert!(loaded.state.documents.len() >= 100);
        assert!(elapsed < Duration::from_secs(5), "Large session serialization took {:?}", elapsed);
    }
}
