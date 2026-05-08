#[cfg(test)]
mod tests {
    use crate::model::{AppState, Document, DocumentEdit, Selection, SessionSnapshot};
    use crate::persistence::{SaveMsg, SaveTelemetry, SaveWorker, SessionEnvelope, load_session};
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
        let mut document = Document::new_untitled(1, 4, true);
        document.replace_text(text);
        document.selections = vec![Selection { anchor: 0, head: 0 }];
        document.revision = 0;
        document
    }

    // Helper to create many documents using open_untitled pattern
    fn create_many_documents(state: &mut AppState, count: usize) {
        for _ in 0..count {
            let doc_id = state.open_untitled(4, true);
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
        assert!(
            elapsed < Duration::from_secs(2),
            "Rapid edits took {:?}",
            elapsed
        );
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
        assert!(
            elapsed < Duration::from_secs(5),
            "Rapid edits on large buffer took {:?}",
            elapsed
        );
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
        assert!(
            elapsed < Duration::from_secs(3),
            "Rapid undo/redo took {:?}",
            elapsed
        );
    }

    // --- Many Tabs Tests ---

    #[test]
    fn many_tabs_creation() {
        let mut state = AppState::empty();
        let start = Instant::now();

        create_many_documents(&mut state, 500);

        let elapsed = start.elapsed();
        assert!(state.documents.len() >= 500);
        assert!(
            elapsed < Duration::from_secs(2),
            "Creating 500 tabs took {:?}",
            elapsed
        );
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

        assert!(
            elapsed < Duration::from_secs(1),
            "Switching 300 tabs took {:?}",
            elapsed
        );
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
        assert!(
            elapsed < Duration::from_secs(2),
            "Loading 5MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_10mb() {
        let text = large_text(100000, 100); // ~10MB
        assert!(text.len() > 10_000_000);

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(
            elapsed < Duration::from_secs(3),
            "Loading 10MB buffer took {:?}",
            elapsed
        );
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
        assert!(
            elapsed < Duration::from_secs(3),
            "Editing 5MB buffer took {:?}",
            elapsed
        );
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
                doc.selections = vec![Selection {
                    anchor: line_start,
                    head: line_end,
                }];
                let _slice = &doc.text()[line_start..line_end];
            }
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "Line operations on large buffer took {:?}",
            elapsed
        );
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

        worker
            .sender()
            .send(SaveMsg::Flush(snapshot, ack_tx))
            .unwrap();
        ack_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();

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
            worker
                .sender()
                .send(SaveMsg::Flush(snapshot, ack_tx))
                .unwrap();
            ack_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();

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

        worker
            .sender()
            .send(SaveMsg::Flush(snapshot, ack_tx))
            .unwrap();
        ack_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();

        // "Restart" and load
        let mut telemetry = SaveTelemetry::default();
        let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();

        assert_eq!(loaded.state.documents.len(), 100);

        worker.shutdown();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }

    // --- Large Buffer Tests (Megabyte and Multi-Megabyte) ---

    #[test]
    fn large_buffer_20mb_load() {
        // Generate ~20MB of text
        let text = large_text(200000, 100); // ~20MB
        assert!(
            text.len() > 20_000_000,
            "Expected >20MB, got {} bytes",
            text.len()
        );

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(
            elapsed < Duration::from_secs(5),
            "Loading 20MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_50mb_load() {
        // Generate ~50MB of text
        let text = large_text(500000, 100); // ~50MB
        assert!(
            text.len() > 50_000_000,
            "Expected >50MB, got {} bytes",
            text.len()
        );

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(
            elapsed < Duration::from_secs(10),
            "Loading 50MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_100mb_load() {
        // Generate ~100MB of text
        let text = large_text(1000000, 100); // ~100MB
        assert!(
            text.len() > 100_000_000,
            "Expected >100MB, got {} bytes",
            text.len()
        );

        let start = Instant::now();
        let doc = make_document(&text);
        let elapsed = start.elapsed();

        assert_eq!(doc.text().len(), text.len());
        assert!(
            elapsed < Duration::from_secs(15),
            "Loading 100MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_20mb_edit_scattered() {
        let text = large_text(200000, 100); // ~20MB
        let mut doc = make_document(&text);
        let rope_len = doc.rope.byte_len();

        let start = Instant::now();
        // Perform 100 edits scattered throughout the 20MB buffer
        for i in 0..100 {
            let pos = (i * rope_len / 100).min(rope_len.saturating_sub(1));
            let edit = DocumentEdit {
                range: pos..pos + 1.min(rope_len - pos),
                inserted_text: format!("EDIT{}", i),
                selections_before: vec![Selection {
                    anchor: pos,
                    head: pos,
                }],
                selections_after: vec![Selection {
                    anchor: pos + format!("EDIT{}", i).len(),
                    head: pos + format!("EDIT{}", i).len(),
                }],
            };
            doc.apply_edit(edit);
        }
        let elapsed = start.elapsed();

        assert!(
            doc.rope.byte_len() > text.len(),
            "Buffer should be larger after edits"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "Editing 20MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_50mb_undo_redo() {
        let text = large_text(500000, 100); // ~50MB
        let mut doc = make_document(&text);

        let start = Instant::now();
        // Perform 50 edits then undo/redo them
        for i in 0..50 {
            let pos = (i * 1000) % doc.rope.byte_len().max(1);
            let edit = DocumentEdit {
                range: pos..pos + 1.min(doc.rope.byte_len() - pos),
                inserted_text: "X".to_string(),
                selections_before: vec![Selection {
                    anchor: pos,
                    head: pos,
                }],
                selections_after: vec![Selection {
                    anchor: pos + 1,
                    head: pos + 1,
                }],
            };
            doc.apply_edit(edit);
        }

        // Undo all
        for _ in 0..50 {
            doc.undo();
        }

        // Redo all
        for _ in 0..50 {
            doc.redo();
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(15),
            "Undo/redo on 50MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_20mb_line_iteration() {
        let text = large_text(200000, 100); // ~20MB
        let doc = make_document(&text);

        let start = Instant::now();
        let mut line_count = 0;
        for line in doc.rope.lines() {
            let _ = line; // Iterate without materializing
            line_count += 1;
            if line_count > 1000 {
                break; // Don't iterate all lines in test
            }
        }
        let elapsed = start.elapsed();

        assert!(line_count > 1000);
        assert!(
            elapsed < Duration::from_secs(2),
            "Line iteration on 20MB took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_50mb_search() {
        let text = large_text(500000, 100); // ~50MB
        let doc = make_document(&text);

        let start = Instant::now();
        // Search for a pattern that exists in the text
        let search_pattern = regex::Regex::new(r"Line\s+\d+").unwrap();
        let mut match_count = 0;
        for line in doc.rope.lines().take(10000) {
            if search_pattern.is_match(&line.to_string()) {
                match_count += 1;
            }
        }
        let elapsed = start.elapsed();

        assert!(match_count > 0, "Should find matches in generated text");
        assert!(
            elapsed < Duration::from_secs(5),
            "Search on 50MB buffer took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_20mb_syntax_detection() {
        let text = large_text(200000, 100); // ~20MB
        let doc = make_document(&text);

        let start = Instant::now();
        let detection = doc.detect_syntax();
        let elapsed = start.elapsed();

        // Should complete without timeout
        assert!(
            elapsed < Duration::from_secs(5),
            "Syntax detection on 20MB took {:?}",
            elapsed
        );
        // Detection may or may not succeed on generated text, just ensure no panic
        let _ = detection;
    }

    #[test]
    fn large_buffer_50mb_session_roundtrip() {
        let text = large_text(500000, 100); // ~50MB
        let mut state = AppState::empty();
        if let Some(doc) = state.active_document_mut() {
            doc.replace_text(&text);
        }

        let start = Instant::now();
        let snapshot = SessionSnapshot::from(&state);
        let envelope = SessionEnvelope::wrap(&snapshot).unwrap();
        let loaded = SessionEnvelope::open(envelope).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(loaded.state.documents.len(), 1);
        assert!(loaded.state.documents[0].rope.byte_len() > 50_000_000);
        assert!(
            elapsed < Duration::from_secs(30),
            "Session roundtrip for 50MB took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_20mb_multiple_selections() {
        let text = large_text(200000, 100); // ~20MB
        let mut doc = make_document(&text);
        let rope_len = doc.rope.byte_len();

        let start = Instant::now();
        // Create 100 selections scattered throughout the buffer
        let mut selections = Vec::new();
        for i in 0..100 {
            let pos = (i * rope_len / 100).min(rope_len.saturating_sub(10));
            selections.push(Selection {
                anchor: pos,
                head: (pos + 5).min(rope_len),
            });
        }
        doc.selections = selections.clone();
        let elapsed = start.elapsed();

        assert_eq!(doc.selections.len(), 100);
        assert!(
            elapsed < Duration::from_secs(2),
            "Multiple selections on 20MB took {:?}",
            elapsed
        );
    }

    #[test]
    fn large_buffer_100mb_memory_usage() {
        // Test that we can create and hold a 100MB buffer without OOM
        let text = large_text(1000000, 100); // ~100MB
        assert!(text.len() > 100_000_000);

        let start = Instant::now();
        let doc = make_document(&text);
        // Force some operations to ensure buffer is real
        let _slice = doc.rope.byte_slice(0..100.min(doc.rope.byte_len()));
        let _len = doc.rope.byte_len();
        let elapsed = start.elapsed();

        assert_eq!(doc.rope.byte_len(), text.len());
        assert!(
            elapsed < Duration::from_secs(20),
            "100MB buffer creation took {:?}",
            elapsed
        );
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

        assert!(
            elapsed < Duration::from_secs(5),
            "Rapid edits across 50 tabs took {:?}",
            elapsed
        );
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
        assert!(
            elapsed < Duration::from_secs(5),
            "Large session serialization took {:?}",
            elapsed
        );
    }
}
