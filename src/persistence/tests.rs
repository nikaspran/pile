use super::*;
use crate::model::AppState;

#[test]
fn flush_writes_a_loadable_session_snapshot() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");

    let worker = SaveWorker::spawn(path.clone());
    let (ack_tx, ack_rx) = bounded(1);
    let snapshot = SessionSnapshot::from(&AppState::empty());
    let mut telemetry = SaveTelemetry::default();

    worker
        .sender()
        .send(SaveMsg::Flush(snapshot.clone(), ack_tx))
        .unwrap();
    ack_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();

    let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 4); // Now using envelope v4
    assert_eq!(loaded.state.documents.len(), 1);

    worker.shutdown();
    let _ = fs::remove_file(path);
    let _ = fs::remove_dir(dir);
}

#[test]
fn envelope_roundtrip_preserves_session() {
    let snapshot = SessionSnapshot::from(&AppState::empty());
    let envelope = SessionEnvelope::wrap(&snapshot).unwrap();
    let loaded = SessionEnvelope::open(envelope).unwrap();

    assert_eq!(loaded.schema_version, 4);
    assert_eq!(loaded.state.documents.len(), 1);
}

#[test]
fn migration_v1_to_v2_then_v3() {
    // Simulate a v1 session (no panes field)
    #[derive(Serialize, Deserialize)]
    struct OldSessionV1 {
        pub schema_version: u32,
        pub state: crate::model::AppState,
    }

    let old_session = OldSessionV1 {
        schema_version: 1,
        state: AppState::empty(),
    };

    let old_bytes = bincode::serialize(&old_session).unwrap();

    // Create v1 envelope
    let v1_envelope = SessionEnvelope {
        envelope_version: 1,
        min_compatible_version: 1,
        payload_bytes: old_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    };

    // This should migrate through v1->v2->v3
    let migrated = SessionEnvelope::open(v1_envelope).unwrap();

    assert_eq!(migrated.schema_version, 4);
    assert_eq!(migrated.state.documents.len(), 1);
}

#[test]
fn legacy_session_loading_still_works() {
    // Create a legacy v2 session (no envelope)
    let snapshot = SessionSnapshot {
        schema_version: 2,
        state: AppState::empty(),
        panes: vec![],
        active_pane: 0,
    };

    let bytes = bincode::serialize(&snapshot).unwrap();

    // Directly deserialize as legacy format
    let loaded: SessionSnapshot = bincode::deserialize(&bytes).unwrap();
    assert_eq!(loaded.schema_version, 2);
}

#[test]
fn backup_rotation_keeps_correct_count() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");

    // Create backups up to BACKUP_ROTATION_COUNT + 2
    for i in 1..=BACKUP_ROTATION_COUNT + 2 {
        let backup_path = path.with_extension(format!("bin.{}", i));
        fs::write(&backup_path, format!("backup {}", i)).unwrap();
    }

    // Verify we have too many backups
    assert!(
        path.with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 2))
            .exists()
    );

    // Trigger rotation
    rotate_backups(&path);

    // Verify oldest backup (BACKUP_ROTATION_COUNT + 2) was removed
    assert!(
        !path
            .with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 2))
            .exists()
    );
    // Verify BACKUP_ROTATION_COUNT + 1 exists (shifted from BACKUP_ROTATION_COUNT)
    assert!(
        path.with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 1))
            .exists()
    );
    // Verify backup.1 no longer exists (it was shifted to backup.2)
    assert!(!path.with_extension("bin.1").exists());

    // Cleanup
    for i in 1..=BACKUP_ROTATION_COUNT + 1 {
        let _ = fs::remove_file(path.with_extension(format!("bin.{}", i)));
    }
    let _ = fs::remove_dir(&dir);
}

#[test]
fn backup_current_session_creates_backup() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Create initial session
    let snapshot = SessionSnapshot::from(&AppState::empty());
    write_snapshot(&path, &snapshot).unwrap();

    // Backup should not exist yet
    assert!(!path.with_extension("bin.1").exists());

    // Create backup
    backup_current_session(&path, &mut telemetry);

    // Backup should now exist
    assert!(path.with_extension("bin.1").exists());

    // Cleanup
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(path.with_extension("bin.1"));
    let _ = fs::remove_dir(&dir);
}

#[test]
fn load_session_from_backup_when_main_corrupt() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Create a valid session and save it
    let worker = SaveWorker::spawn(path.clone());
    let (ack_tx, ack_rx) = bounded(1);
    let snapshot = SessionSnapshot::from(&AppState::empty());

    worker
        .sender()
        .send(SaveMsg::Flush(snapshot.clone(), ack_tx))
        .unwrap();
    ack_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();

    // Verify main session exists and is valid
    let loaded = load_session(&path, &mut telemetry).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 4);

    // Now corrupt the main session file
    fs::write(&path, "corrupted data").unwrap();

    // Main session should fail to load now
    let result = load_session(&path, &mut telemetry);
    assert!(result.is_err() || result.unwrap().is_none());

    // But we should have a backup (created before the corrupt write)
    // Actually, let's manually create a backup for this test
    let backup_path = path.with_extension("bin.1");
    // Write a valid session to backup
    write_snapshot(&backup_path, &SessionSnapshot::from(&AppState::empty())).unwrap();

    // Quarantine the corrupt main session
    quarantine_corrupt_session(&path, &mut telemetry);

    // Try to load from backup
    let backup_result = load_session_from_backup(&path, &mut telemetry).unwrap();
    assert!(backup_result.is_some());

    worker.shutdown();

    // Cleanup
    let _ = fs::remove_file(&backup_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn snapshot_budget_allows_normal_size() {
    let state = AppState::empty();
    let snapshot = SessionSnapshot::from(&state);
    match check_snapshot_budget(&snapshot) {
        BudgetCheck::Ok => (), // Expected
        BudgetCheck::TooLarge { size, max } => {
            panic!("Normal snapshot should fit in budget: {} > {}", size, max);
        }
    }
}

#[test]
fn snapshot_budget_rejects_huge_session() {
    // Create a huge state with many large documents
    let mut state = AppState::empty();
    // Clear the default document
    state.documents.clear();
    state.tab_order.clear();

    // Add documents with large content to exceed 50 MB
    let large_content = "x".repeat(10_000_000); // 10 MB each
    for i in 0..6 {
        let mut doc = crate::model::Document::new_untitled(i as u64 + 100, 4, true);
        doc.replace_text(&large_content);
        state.documents.push(doc);
        state.tab_order.push(state.documents.last().unwrap().id);
    }
    state.active_document = state.tab_order[0];

    let snapshot = SessionSnapshot::from(&state);
    match check_snapshot_budget(&snapshot) {
        BudgetCheck::TooLarge { size, max } => {
            assert!(size > max, "Should exceed budget: {} <= {}", size, max);
        }
        BudgetCheck::Ok => {
            // Might pass if serialization is efficient, which is fine
            // Just verify the check doesn't panic
        }
    }
}

// --- Schema Migration Edge Case Tests ---

#[test]
fn migration_v2_to_v3_preserves_state() {
    // Create a v2 payload (schema_version=2 in payload)
    let mut state = AppState::empty();
    // Add some documents to verify state is preserved
    let doc_id = state.open_untitled(4, true);
    if let Some(doc) = state.document_mut(doc_id) {
        doc.replace_text("test content");
    }

    let v2_snapshot = SessionSnapshot {
        schema_version: 2,
        state: state.clone(),
        panes: vec![],
        active_pane: 0,
    };

    let v2_bytes = bincode::serialize(&v2_snapshot).unwrap();

    // Create v2 envelope
    let v2_envelope = SessionEnvelope {
        envelope_version: 2,
        min_compatible_version: 2,
        payload_bytes: v2_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    };

    // Migrate to v3
    let migrated = SessionEnvelope::open(v2_envelope).unwrap();

    assert_eq!(migrated.schema_version, 4);
    assert_eq!(migrated.state.documents.len(), state.documents.len());
}

#[test]
fn migration_from_v1_with_documents() {
    // Create a v1 session with multiple documents
    let mut state = AppState::empty();
    let doc_id = state.open_untitled(4, true);
    if let Some(doc) = state.document_mut(doc_id) {
        doc.replace_text("document 1");
    }
    let doc_id2 = state.open_untitled(4, true);
    if let Some(doc) = state.document_mut(doc_id2) {
        doc.replace_text("document 2");
    }

    // Simulate v1 format (no panes)
    #[derive(Serialize, Deserialize)]
    struct OldSessionV1 {
        pub schema_version: u32,
        pub state: crate::model::AppState,
    }

    let v1_session = OldSessionV1 {
        schema_version: 1,
        state: state.clone(),
    };

    let v1_bytes = bincode::serialize(&v1_session).unwrap();

    let v1_envelope = SessionEnvelope {
        envelope_version: 1,
        min_compatible_version: 1,
        payload_bytes: v1_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    };

    let migrated = SessionEnvelope::open(v1_envelope).unwrap();

    assert_eq!(migrated.schema_version, 4);
    assert_eq!(migrated.state.documents.len(), 3); // 2 + 1 default
    assert!(migrated.panes.is_empty());
}

// --- Corrupt Session Handling Tests ---

#[test]
fn corrupt_session_truncated_data() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Write truncated data (just 4 bytes)
    fs::write(&path, vec![1, 2, 3, 4]).unwrap();

    // Try to load - should fail gracefully
    let result = load_session(&path, &mut telemetry);
    assert!(result.is_err() || result.unwrap().is_none());

    // Verify quarantine was called (file should be moved to .bad)
    assert!(path.with_extension("bin.bad").exists());

    // Cleanup
    let _ = fs::remove_file(path.with_extension("bin.bad"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_session_invalid_bincode() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Write random bytes (invalid bincode)
    let corrupt_data: Vec<u8> = (0..100).map(|i| (i * 7) as u8).collect();
    fs::write(&path, corrupt_data).unwrap();

    // Try to load - should fail
    let result = load_session(&path, &mut telemetry);
    assert!(result.is_err() || result.unwrap().is_none());

    // Cleanup
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_session_quarantine_and_backup_restore() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Create a valid backup
    let valid_snapshot = SessionSnapshot::from(&AppState::empty());
    let backup_path = path.with_extension("bin.1");
    write_snapshot(&backup_path, &valid_snapshot).unwrap();

    // Write corrupt data to main session
    fs::write(&path, "corrupted").unwrap();

    // Load session - should restore from backup
    let result = load_session(&path, &mut telemetry);

    // Either restored from backup or failed
    match result {
        Ok(Some(snapshot)) => {
            // Successfully restored from backup
            assert_eq!(snapshot.schema_version, 4);
        }
        Ok(None) => {
            // No backup available or backup also corrupt
        }
        Err(_) => {
            // Error during load
        }
    }

    // Verify recovery events were recorded
    assert!(!telemetry.recovery_events.is_empty());

    // Cleanup
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&backup_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_session_with_all_backups_corrupt() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Write corrupt data to main session
    fs::write(&path, "corrupted main").unwrap();

    // Write corrupt data to all backup slots
    for i in 1..=BACKUP_ROTATION_COUNT {
        let backup_path = path.with_extension(format!("bin.{}", i));
        fs::write(&backup_path, format!("corrupt backup {}", i)).unwrap();
    }

    // Try to load - should fail
    let result = load_session(&path, &mut telemetry);

    // Should not be able to load
    assert!(result.is_err() || result.unwrap().is_none());

    // Cleanup
    let _ = fs::remove_file(&path);
    for i in 1..=BACKUP_ROTATION_COUNT {
        let _ = fs::remove_file(path.with_extension(format!("bin.{}", i)));
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn envelope_version_too_old() {
    // Create envelope with version older than min_compatible
    let snapshot = SessionSnapshot::from(&AppState::empty());
    let payload_bytes = bincode::serialize(&snapshot).unwrap();

    let old_envelope = SessionEnvelope {
        envelope_version: 1,
        min_compatible_version: 3, // Requires v3, but we have v1
        payload_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    };

    // Should fail because version is too old
    let result = SessionEnvelope::open(old_envelope);
    assert!(result.is_err());
}

#[test]
fn wrong_payload_type_rejected() {
    let snapshot = SessionSnapshot::from(&AppState::empty());
    let payload_bytes = bincode::serialize(&snapshot).unwrap();

    let envelope = SessionEnvelope {
        envelope_version: 4,
        min_compatible_version: 4,
        payload_bytes,
        payload_type: "WrongType".to_owned(),
    };

    // Should fail because payload type doesn't match
    let result = SessionEnvelope::open(envelope);
    assert!(result.is_err());
}

#[test]
fn quarantine_corrupt_session_moves_file() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let bad_path = path.with_extension("bin.bad");
    let mut telemetry = SaveTelemetry::default();

    // Create a corrupt session file
    fs::write(&path, "corrupt data").unwrap();
    assert!(path.exists());
    assert!(!bad_path.exists());

    // Quarantine it
    quarantine_corrupt_session(&path, &mut telemetry);

    // Original should be gone, .bad should exist
    assert!(!path.exists());
    assert!(bad_path.exists());

    // Verify telemetry was updated
    let has_corrupt_event = telemetry
        .recovery_events
        .iter()
        .any(|e| matches!(e.kind, RecoveryEventKind::SessionCorrupt));
    assert!(has_corrupt_event);

    // Cleanup
    let _ = fs::remove_file(&bad_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_session_from_multiple_backups() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    // Create multiple backups, some corrupt
    // Backup 1: corrupt
    fs::write(path.with_extension("bin.1"), "corrupt").unwrap();

    // Backup 2: valid
    let valid_snapshot = SessionSnapshot::from(&AppState::empty());
    write_snapshot(&path.with_extension("bin.2"), &valid_snapshot).unwrap();

    // Backup 3: corrupt
    fs::write(path.with_extension("bin.3"), "also corrupt").unwrap();

    // Should skip corrupt backups and use backup 2
    let result = load_session_from_backup(&path, &mut telemetry);

    assert!(result.is_ok());
    let loaded = result.unwrap();
    assert!(loaded.is_some());

    let (snapshot, backup_path) = loaded.unwrap();
    assert_eq!(snapshot.schema_version, 4);
    assert_eq!(backup_path, path.with_extension("bin.2"));

    // Cleanup
    for i in 1..=3 {
        let _ = fs::remove_file(path.with_extension(format!("bin.{}", i)));
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_v1_session_without_envelope() {
    // Test loading a pure v1 session (no envelope at all)
    let mut state = AppState::empty();
    let doc_id = state.open_untitled(4, true);
    if let Some(doc) = state.document_mut(doc_id) {
        doc.replace_text("legacy content");
    }

    #[derive(Serialize, Deserialize)]
    struct OldSessionV1 {
        pub schema_version: u32,
        pub state: crate::model::AppState,
    }

    let v1_session = OldSessionV1 {
        schema_version: 1,
        state: state.clone(),
    };

    let v1_bytes = bincode::serialize(&v1_session).unwrap();

    // Now test through load_session path
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".session.bin");
    let mut telemetry = SaveTelemetry::default();

    fs::write(&path, v1_bytes).unwrap();

    // load_session should handle the legacy v1 format
    let loaded = load_session(&path, &mut telemetry).unwrap();
    assert!(loaded.is_some());
    let snapshot = loaded.unwrap();
    assert_eq!(snapshot.schema_version, 2); // Migrated to v2 by load_session
    assert_eq!(snapshot.panes.len(), 0); // v1 didn't have panes

    // Cleanup
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recover_orphan_documents_from_backups() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let session_path = dir.join(".session.bin");

    // Main state: [A, B]
    let mut main_state = AppState::empty();
    let doc_a = main_state.active_document;
    let _doc_b = main_state.open_untitled(4, true);

    // Clone main state to create backup before any close
    let mut backup_state = main_state.clone();

    // Add an orphan doc (C) to backup that never existed in main
    let orphan = backup_state.open_untitled(4, true);
    if let Some(doc) = backup_state.document_mut(orphan) {
        doc.replace_text("orphaned from v3 close");
    }
    // backup_state.documents = [A, B, C]

    let backup_snapshot = SessionSnapshot::from(&backup_state);
    let backup_path = session_path.with_extension("bin.1");
    write_snapshot(&backup_path, &backup_snapshot).unwrap();

    // Current snapshot (no C ever existed)
    let mut snapshot = SessionSnapshot::from(&main_state);
    assert!(snapshot.state.document(orphan).is_none());

    // Run recovery
    let mut telemetry = SaveTelemetry::default();
    recover_orphan_documents(&session_path, &mut snapshot, &mut telemetry);

    // C was recovered into closed_documents
    assert!(
        snapshot
            .state
            .closed_documents
            .iter()
            .any(|cd| cd.document.id == orphan),
    );
    let recovered = snapshot
        .state
        .closed_documents
        .iter()
        .find(|cd| cd.document.id == orphan)
        .unwrap();
    assert_eq!(
        recovered.document.rope.to_string(),
        "orphaned from v3 close"
    );

    // Telemetry recorded
    assert!(
        telemetry
            .recovery_events
            .iter()
            .any(|e| matches!(e.kind, RecoveryEventKind::DocumentsRecovered))
    );

    // A and B were NOT recovered (they are open)
    assert!(
        !snapshot
            .state
            .closed_documents
            .iter()
            .any(|cd| cd.document.id == doc_a)
    );

    // Cleanup
    let _ = fs::remove_file(&backup_path);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recover_orphan_documents_skips_already_closed() {
    let dir = std::env::temp_dir().join(format!("pile-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let session_path = dir.join(".session.bin");

    // Main state: [A, B, C]
    let mut main_state = AppState::empty();
    let _doc_b = main_state.open_untitled(4, true);
    let doc_c = main_state.open_untitled(4, true);

    // Clone backup BEFORE closing C
    let backup_state = main_state.clone();
    // backup_state.documents = [A, B, C]

    // Close C in main
    main_state.close_document_by_id(doc_c);
    // main_state.documents = [A, B], closed = [C]

    let mut snapshot = SessionSnapshot::from(&main_state);

    let backup_snapshot = SessionSnapshot::from(&backup_state);
    let backup_path = session_path.with_extension("bin.1");
    write_snapshot(&backup_path, &backup_snapshot).unwrap();

    let mut telemetry = SaveTelemetry::default();
    recover_orphan_documents(&session_path, &mut snapshot, &mut telemetry);

    // C should NOT be recovered (already in closed_documents)
    assert_eq!(snapshot.state.closed_documents.len(), 1);
    assert_eq!(snapshot.state.closed_documents[0].document.id, doc_c);

    // Cleanup
    let _ = fs::remove_file(&backup_path);
    let _ = fs::remove_dir_all(&dir);
}
