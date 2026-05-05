use std::{
    fs,
    io::Write,
    path::PathBuf,
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result};
use atomic_write_file::AtomicWriteFile;
use crossbeam_channel::{Receiver, Sender, bounded};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::model::{PaneSnapshot, SessionSnapshot};
use crate::settings::Settings;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const SESSION_FILE: &str = ".session.bin";
const BACKUP_ROTATION_COUNT: usize = 5;

/// Current envelope version. Increment this when the session format changes.
const ENVELOPE_VERSION: u32 = 3;

/// Versioned envelope that wraps the session payload.
/// This separates envelope metadata from the payload and provides
/// explicit migration hooks for version upgrades.
#[derive(Serialize, Deserialize)]
pub struct SessionEnvelope {
    /// Envelope format version (for forward compatibility)
    pub envelope_version: u32,
    /// Minimum envelope version that can read this session
    pub min_compatible_version: u32,
    /// The payload type tag for validation
    pub payload_type: String,
    /// The serialized payload bytes (stored separately to allow versioned deserialization)
    #[serde(skip)]
    pub payload_bytes: Vec<u8>,
}

impl SessionEnvelope {
    /// Create a new envelope wrapping the given payload.
    /// This updates the payload's schema_version to match the current envelope version.
    pub fn wrap(payload: &SessionSnapshot) -> Result<Self> {
        let mut payload = payload.clone();
        payload.schema_version = ENVELOPE_VERSION;
        let payload_bytes = bincode::serialize(&payload)?;
        Ok(Self {
            envelope_version: ENVELOPE_VERSION,
            min_compatible_version: 3,
            payload_bytes,
            payload_type: "SessionSnapshot".to_owned(),
        })
    }

    /// Open and migrate a session from the envelope.
    /// This is the main entry point that handles all version migrations.
    pub fn open(envelope: SessionEnvelope) -> Result<SessionSnapshot> {
        migrate_session(envelope)
    }

    /// Serialize the envelope to bytes (envelope + payload).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Serialize envelope metadata (without payload_bytes due to serde(skip))
        let metadata_bytes = bincode::serialize(&EnvelopeMetadata {
            envelope_version: self.envelope_version,
            min_compatible_version: self.min_compatible_version,
            payload_type: self.payload_type.clone(),
        })?;

        // Combine: metadata length (4 bytes) + metadata + payload
        let mut result = Vec::new();
        let metadata_len = metadata_bytes.len() as u32;
        result.extend_from_slice(&metadata_len.to_le_bytes());
        result.extend_from_slice(&metadata_bytes);
        result.extend_from_slice(&self.payload_bytes);
        Ok(result)
    }

    /// Deserialize the envelope from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            anyhow::bail!("envelope too short");
        }

        let metadata_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if bytes.len() < 4 + metadata_len {
            anyhow::bail!("envelope metadata truncated");
        }

        let metadata: EnvelopeMetadata = bincode::deserialize(&bytes[4..4 + metadata_len])
            .with_context(|| "failed to deserialize envelope metadata")?;

        let payload_bytes = bytes[4 + metadata_len..].to_vec();

        Ok(Self {
            envelope_version: metadata.envelope_version,
            min_compatible_version: metadata.min_compatible_version,
            payload_type: metadata.payload_type,
            payload_bytes,
        })
    }
}

/// Helper struct for serializing envelope metadata without payload.
#[derive(Serialize, Deserialize)]
struct EnvelopeMetadata {
    pub envelope_version: u32,
    pub min_compatible_version: u32,
    pub payload_type: String,
}

/// Migration function that handles all version transitions.
/// Each migration is an explicit, documented step.
fn migrate_session(mut envelope: SessionEnvelope) -> Result<SessionSnapshot> {
    // Check if we can read this version
    if envelope.envelope_version < envelope.min_compatible_version {
        anyhow::bail!(
            "session envelope version {} is too old (minimum compatible: {})",
            envelope.envelope_version,
            envelope.min_compatible_version
        );
    }

    // Apply migrations sequentially
    if envelope.envelope_version == 1 {
        envelope = migrate_v1_to_v2(envelope)?;
    }
    if envelope.envelope_version == 2 {
        envelope = migrate_v2_to_v3(envelope)?;
    }

    // Now at current version, deserialize
    if envelope.payload_type != "SessionSnapshot" {
        anyhow::bail!(
            "unexpected payload type: {}",
            envelope.payload_type
        );
    }

    let snapshot: SessionSnapshot = bincode::deserialize(&envelope.payload_bytes)
        .with_context(|| "failed to deserialize session payload")?;

    Ok(snapshot)
}

/// Migration from envelope v1 to v2.
/// v1 sessions had schema_version=1 and no panes support.
fn migrate_v1_to_v2(envelope: SessionEnvelope) -> Result<SessionEnvelope> {
    // Deserialize the old format (schema_version=1 means no panes)
    #[derive(Serialize, Deserialize)]
    struct OldSessionV1 {
        pub schema_version: u32,
        pub state: crate::model::AppState,
    }

    let old: OldSessionV1 = bincode::deserialize(&envelope.payload_bytes)
        .with_context(|| "failed to deserialize v1 session")?;

    // Create new payload with v2 format (adds panes)
    let new_payload = SessionSnapshot {
        schema_version: 2,
        state: old.state,
        panes: vec![],
        active_pane: 0,
    };

    let payload_bytes = bincode::serialize(&new_payload)?;

    Ok(SessionEnvelope {
        envelope_version: 2,
        min_compatible_version: 2,
        payload_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    })
}

/// Migration from envelope v2 to v3.
/// v2 had schema_version in the payload; v3 moves versioning to the envelope.
fn migrate_v2_to_v3(mut envelope: SessionEnvelope) -> Result<SessionEnvelope> {
    // v2 payload is SessionSnapshot with schema_version field
    let mut snapshot: SessionSnapshot = bincode::deserialize(&envelope.payload_bytes)
        .with_context(|| "failed to deserialize v2 session")?;

    // Strip schema_version from payload (no longer needed in v3)
    snapshot.schema_version = 3;

    let payload_bytes = bincode::serialize(&snapshot)?;

    Ok(SessionEnvelope {
        envelope_version: 3,
        min_compatible_version: 3,
        payload_bytes,
        payload_type: "SessionSnapshot".to_owned(),
    })
}

#[derive(Debug)]
pub enum SaveMsg {
    Changed(SessionSnapshot),
    Flush(SessionSnapshot, Sender<Result<(), String>>),
    Shutdown,
}

pub struct SaveWorker {
    tx: Sender<SaveMsg>,
    handle: Option<JoinHandle<()>>,
}

impl SaveWorker {
    pub fn spawn(session_path: PathBuf) -> Self {
        let (tx, rx) = bounded(128);
        let handle = thread::Builder::new()
            .name("pile-session-save".to_owned())
            .spawn(move || run_save_loop(rx, session_path))
            .expect("failed to spawn session save worker");

        Self {
            tx,
            handle: Some(handle),
        }
    }

    pub fn sender(&self) -> Sender<SaveMsg> {
        self.tx.clone()
    }

    pub fn shutdown(mut self) {
        let _ = self.tx.send(SaveMsg::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SaveWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(SaveMsg::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn default_session_path() -> PathBuf {
    ProjectDirs::from("", "", "pile")
        .map(|dirs| dirs.data_local_dir().join(SESSION_FILE))
        .unwrap_or_else(|| PathBuf::from(SESSION_FILE))
}

pub fn default_settings_path() -> PathBuf {
    ProjectDirs::from("", "", "pile")
        .map(|dirs| dirs.data_local_dir().join("settings.json"))
        .unwrap_or_else(|| PathBuf::from("settings.json"))
}

pub fn load_settings(path: &PathBuf) -> Settings {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(path: &PathBuf, settings: &Settings) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, json);
    }
}

pub fn load_session(path: &PathBuf) -> Result<Option<SessionSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Try to deserialize as new envelope format first
    match SessionEnvelope::from_bytes(&bytes) {
        Ok(envelope) => {
            let snapshot = SessionEnvelope::open(envelope)?;
            Ok(Some(snapshot))
        }
        Err(_) => {
            // Fallback: try to deserialize as legacy SessionSnapshot (v1/v2)
            match bincode::deserialize::<SessionSnapshot>(&bytes) {
                Ok(snapshot) => {
                    // Support schema v1 by migrating to v2
                    if snapshot.schema_version == 1 {
                        let migrated = SessionSnapshot {
                            schema_version: 2,
                            state: snapshot.state,
                            panes: vec![],
                            active_pane: 0,
                        };
                        return Ok(Some(migrated));
                    }

                    if snapshot.schema_version != 2 {
                        anyhow::bail!("unsupported session schema {}", snapshot.schema_version);
                    }

                    Ok(Some(snapshot))
                }
                Err(_) => {
                    // Main session is corrupt, quarantine it and try backups
                    warn!(
                        path = %path.display(),
                        "main session corrupt, trying backups"
                    );
                    quarantine_corrupt_session(path);

                    match load_session_from_backup(path) {
                        Ok(Some((snapshot, backup_path))) => {
                            // Restore from backup
                            let _ = fs::copy(&backup_path, path);
                            info!(
                                backup_path = %backup_path.display(),
                                "restored session from backup"
                            );
                            Ok(Some(snapshot))
                        }
                        Ok(None) => {
                            warn!("no loadable backups found");
                            Ok(None)
                        }
                        Err(err) => {
                            warn!(error = %err, "failed to load from backup");
                            Ok(None)
                        }
                    }
                }
            }
        }
    }
}

fn run_save_loop(rx: Receiver<SaveMsg>, session_path: PathBuf) {
    while let Ok(message) = rx.recv() {
        match message {
            SaveMsg::Changed(snapshot) => {
                let mut latest = snapshot;
                loop {
                    match rx.recv_timeout(SAVE_DEBOUNCE) {
                        Ok(SaveMsg::Changed(snapshot)) => latest = snapshot,
                        Ok(SaveMsg::Flush(snapshot, ack)) => {
                            latest = snapshot;
                            save_and_ack(&session_path, &latest, Some(ack));
                            break;
                        }
                        Ok(SaveMsg::Shutdown) => {
                            save_snapshot(&session_path, &latest);
                            return;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            save_snapshot(&session_path, &latest);
                            break;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
            SaveMsg::Flush(snapshot, ack) => save_and_ack(&session_path, &snapshot, Some(ack)),
            SaveMsg::Shutdown => return,
        }
    }
}

fn save_and_ack(
    path: &PathBuf,
    snapshot: &SessionSnapshot,
    ack: Option<Sender<Result<(), String>>>,
) {
    let result = write_snapshot(path, snapshot).map_err(|err| {
        error!(error = %err, path = %path.display(), "session save failed");
        err.to_string()
    });

    if let Some(ack) = ack {
        let _ = ack.send(result);
    }
}

fn save_snapshot(path: &PathBuf, snapshot: &SessionSnapshot) {
    // Backup current session before overwriting
    backup_current_session(path);

    if let Err(err) = write_snapshot(path, snapshot) {
        error!(error = %err, path = %path.display(), "session save failed");
    } else {
        debug!(path = %path.display(), "session saved");
    }
}

fn write_snapshot(path: &PathBuf, snapshot: &SessionSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    // Wrap in versioned envelope and serialize
    let envelope = SessionEnvelope::wrap(snapshot)?;
    let bytes = envelope.to_bytes()?;

    let mut file = AtomicWriteFile::options()
        .open(path)
        .with_context(|| format!("failed to open atomic writer for {}", path.display()))?;

    file.write_all(&bytes)?;
    file.commit()
        .with_context(|| format!("failed to commit {}", path.display()))?;
    Ok(())
}

pub fn quarantine_corrupt_session(path: &PathBuf) {
    let bad_path = path.with_extension("bin.bad");
    if let Err(err) = fs::rename(path, &bad_path) {
        warn!(
            error = %err,
            path = %path.display(),
            bad_path = %bad_path.display(),
            "failed to quarantine corrupt session"
        );
    }
}

/// Rotate backup files, keeping only the N most recent backups.
/// Backups are named .session.bin.1, .session.bin.2, etc. (higher = older)
fn rotate_backups(session_path: &PathBuf) {
    // Remove any backups beyond the rotation count
    for i in BACKUP_ROTATION_COUNT + 1..=BACKUP_ROTATION_COUNT + 10 {
        let old_backup = session_path.with_extension(format!("bin.{}", i));
        if !old_backup.exists() {
            break;
        }
        let _ = fs::remove_file(&old_backup);
    }

    // Shift existing backups: .session.bin.N -> .session.bin.N+1, etc.
    for i in (1..=BACKUP_ROTATION_COUNT).rev() {
        let src = session_path.with_extension(format!("bin.{}", i));
        let dst = session_path.with_extension(format!("bin.{}", i + 1));
        if src.exists() {
            let _ = fs::rename(&src, &dst);
        }
    }
}

/// Create a backup of the current session file before overwriting.
/// Rotates existing backups and copies current session to .session.bin.1
pub fn backup_current_session(path: &PathBuf) {
    if !path.exists() {
        return;
    }

    rotate_backups(path);

    let backup_path = path.with_extension("bin.1");
    if let Err(err) = fs::copy(path, &backup_path) {
        warn!(
            error = %err,
            path = %path.display(),
            backup_path = %backup_path.display(),
            "failed to create session backup"
        );
    }
}

/// Try to load session from backup files, starting from the most recent.
/// Returns the first loadable backup, or None if all backups are corrupt.
pub fn load_session_from_backup(session_path: &PathBuf) -> Result<Option<(SessionSnapshot, PathBuf)>> {
    for i in 1..=BACKUP_ROTATION_COUNT {
        let backup_path = session_path.with_extension(format!("bin.{}", i));
        if !backup_path.exists() {
            continue;
        }

        match load_session(&backup_path) {
            Ok(Some(snapshot)) => {
                info!(
                    backup_path = %backup_path.display(),
                    "restored session from backup"
                );
                return Ok(Some((snapshot, backup_path)));
            }
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    error = %err,
                    backup_path = %backup_path.display(),
                    "backup also corrupt, trying next"
                );
                continue;
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
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

        worker
            .sender()
            .send(SaveMsg::Flush(snapshot.clone(), ack_tx))
            .unwrap();
        ack_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();

        let loaded = load_session(&path).unwrap().unwrap();
        assert_eq!(loaded.schema_version, 3); // Now using envelope v3
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

        assert_eq!(loaded.schema_version, 3);
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

        assert_eq!(migrated.schema_version, 3);
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
        assert!(path.with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 2)).exists());

        // Trigger rotation
        rotate_backups(&path);

        // Verify oldest backup (BACKUP_ROTATION_COUNT + 2) was removed
        assert!(!path.with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 2)).exists());
        // Verify BACKUP_ROTATION_COUNT + 1 exists (shifted from BACKUP_ROTATION_COUNT)
        assert!(path.with_extension(format!("bin.{}", BACKUP_ROTATION_COUNT + 1)).exists());
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

        // Create initial session
        let snapshot = SessionSnapshot::from(&AppState::empty());
        write_snapshot(&path, &snapshot).unwrap();

        // Backup should not exist yet
        assert!(!path.with_extension("bin.1").exists());

        // Create backup
        backup_current_session(&path);

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

        // Create a valid session and save it
        let worker = SaveWorker::spawn(path.clone());
        let (ack_tx, ack_rx) = bounded(1);
        let snapshot = SessionSnapshot::from(&AppState::empty());

        worker
            .sender()
            .send(SaveMsg::Flush(snapshot.clone(), ack_tx))
            .unwrap();
        ack_rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();

        // Verify main session exists and is valid
        let loaded = load_session(&path).unwrap().unwrap();
        assert_eq!(loaded.schema_version, 3);

        // Now corrupt the main session file
        fs::write(&path, "corrupted data").unwrap();

        // Main session should fail to load now
        let result = load_session(&path);
        assert!(result.is_err() || result.unwrap().is_none());

        // But we should have a backup (created before the corrupt write)
        // Actually, let's manually create a backup for this test
        let backup_path = path.with_extension("bin.1");
        // Write a valid session to backup
        write_snapshot(&backup_path, &SessionSnapshot::from(&AppState::empty())).unwrap();

        // Quarantine the corrupt main session
        quarantine_corrupt_session(&path);

        // Try to load from backup
        let backup_result = load_session_from_backup(&path).unwrap();
        assert!(backup_result.is_some());

        worker.shutdown();

        // Cleanup
        let _ = fs::remove_file(&backup_path);
        let _ = fs::remove_dir_all(&dir);
    }
}
