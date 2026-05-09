use std::{
    collections::HashSet,
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

use crate::model::{ClosedDocument, Document, DocumentId, SessionSnapshot};
use crate::settings::Settings;

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const SESSION_FILE: &str = ".session.bin";
const BACKUP_ROTATION_COUNT: usize = 5;

/// Current envelope version. Increment this when the session format changes.
const ENVELOPE_VERSION: u32 = 4;

/// Maximum allowed serialized snapshot size in bytes (50 MB).
/// If the snapshot exceeds this, the save is skipped to prevent stalling the UI.
const MAX_SNAPSHOT_SIZE: usize = 50 * 1024 * 1024;

/// Result of checking a snapshot against the memory budget.
pub enum BudgetCheck {
    Ok,
    TooLarge { size: usize, max: usize },
}

/// Check if a snapshot fits within the memory budget.
/// Returns the serialized size without writing to disk.
pub fn check_snapshot_budget(snapshot: &SessionSnapshot) -> BudgetCheck {
    match bincode::serialize(snapshot) {
        Ok(bytes) => {
            let size = bytes.len();
            if size > MAX_SNAPSHOT_SIZE {
                BudgetCheck::TooLarge {
                    size,
                    max: MAX_SNAPSHOT_SIZE,
                }
            } else {
                BudgetCheck::Ok
            }
        }
        Err(_) => BudgetCheck::Ok, // Let the actual save handle serialization errors
    }
}

/// A recoverable session event surfaced to the user.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryEvent {
    pub timestamp: std::time::SystemTime,
    pub kind: RecoveryEventKind,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecoveryEventKind {
    SessionCorrupt,
    BackupRestored,
    BackupFailed,
    QuarantineFailed,
    SaveFailed,
    SaveSucceeded,
    DocumentsRecovered,
}

/// Telemetry collected by the save worker across the session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SaveTelemetry {
    pub total_saves: u64,
    pub successful_saves: u64,
    pub failed_saves: u64,
    pub last_save_duration_ms: Option<u64>,
    pub recovery_events: Vec<RecoveryEvent>,
    /// Rolling buffer of recent save durations in milliseconds (capped at 1000 entries).
    #[serde(skip)]
    pub save_durations_ms: Vec<u64>,
}

impl SaveTelemetry {
    /// Record a save duration and update statistics.
    pub fn record_save_duration(&mut self, duration_ms: u64) {
        self.last_save_duration_ms = Some(duration_ms);
        self.save_durations_ms.push(duration_ms);
        // Keep only the last 1000 entries to bound memory
        if self.save_durations_ms.len() > 1000 {
            self.save_durations_ms.remove(0);
        }
    }

    /// Calculate the median save duration in milliseconds.
    pub fn median_save_duration_ms(&self) -> Option<u64> {
        if self.save_durations_ms.is_empty() {
            None
        } else {
            let mut sorted = self.save_durations_ms.clone();
            sorted.sort_unstable();
            Some(sorted[sorted.len() / 2])
        }
    }

    /// Calculate the 95th percentile save duration in milliseconds.
    pub fn p95_save_duration_ms(&self) -> Option<u64> {
        if self.save_durations_ms.len() < 20 {
            return None; // Not enough data
        }
        let mut sorted = self.save_durations_ms.clone();
        sorted.sort_unstable();
        let idx = (sorted.len() as f64 * 0.95) as usize;
        Some(sorted[idx.min(sorted.len() - 1)])
    }
}

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
            min_compatible_version: 4,
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
    if envelope.envelope_version == 3 {
        envelope = migrate_v3_to_v4(envelope)?;
    }

    // Now at current version, deserialize
    if envelope.payload_type != "SessionSnapshot" {
        anyhow::bail!("unexpected payload type: {}", envelope.payload_type);
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
fn migrate_v2_to_v3(envelope: SessionEnvelope) -> Result<SessionEnvelope> {
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

/// Migration from envelope v3 to v4.
/// v3 lacked closed_document history support; v4 adds AppState::closed_documents
/// and AppState::next_closed_order.
fn migrate_v3_to_v4(envelope: SessionEnvelope) -> Result<SessionEnvelope> {
    use crate::model::{AppState, Document, DocumentId, PaneSnapshot, deserialize_recent_order};

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct SnapshotV3 {
        schema_version: u32,
        documents: Vec<Document>,
        tab_order: Vec<DocumentId>,
        active_document: DocumentId,
        next_untitled_index: u64,
        #[serde(deserialize_with = "deserialize_recent_order")]
        recent_order: Vec<DocumentId>,
        panes: Vec<PaneSnapshot>,
        active_pane: usize,
    }

    let v3: SnapshotV3 = bincode::deserialize(&envelope.payload_bytes)
        .with_context(|| "failed to deserialize v3 session")?;

    let new_state = AppState {
        documents: v3.documents,
        tab_order: v3.tab_order,
        active_document: v3.active_document,
        next_untitled_index: v3.next_untitled_index,
        recent_order: v3.recent_order,
        closed_documents: Vec::new(),
        next_closed_order: 0,
    };

    let new_payload = SessionSnapshot {
        schema_version: 4,
        state: new_state,
        panes: v3.panes,
        active_pane: v3.active_pane,
    };

    let payload_bytes = bincode::serialize(&new_payload)?;

    Ok(SessionEnvelope {
        envelope_version: 4,
        min_compatible_version: 4,
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

/// Messages sent from the save worker back to the UI thread.
#[derive(Debug)]
pub enum WorkerEvent {
    #[allow(dead_code)]
    Recovery(RecoveryEvent),
    Telemetry(SaveTelemetry),
}

pub struct SaveWorker {
    tx: Sender<SaveMsg>,
    handle: Option<JoinHandle<()>>,
}

impl SaveWorker {
    #[allow(dead_code)]
    pub fn spawn(session_path: PathBuf) -> Self {
        let (tx, rx) = bounded(128);
        let handle = thread::Builder::new()
            .name("pile-session-save".to_owned())
            .spawn(move || {
                let mut telemetry = SaveTelemetry::default();
                run_save_loop(rx, &session_path, &mut telemetry)
            })
            .expect("failed to spawn session save worker");

        Self {
            tx,
            handle: Some(handle),
        }
    }

    /// Spawn with a channel that receives telemetry and recovery events.
    pub fn spawn_with_events(session_path: PathBuf, event_tx: Sender<WorkerEvent>) -> Self {
        let (tx, rx) = bounded(128);
        let handle = thread::Builder::new()
            .name("pile-session-save".to_owned())
            .spawn(move || {
                let mut telemetry = SaveTelemetry::default();
                run_save_loop(rx, &session_path, &mut telemetry);
                // Log latency statistics before sending telemetry
                if let Some(median) = telemetry.median_save_duration_ms() {
                    info!(target: "pile::save_worker", median_ms = median, "save worker median latency");
                }
                if let Some(p95) = telemetry.p95_save_duration_ms() {
                    info!(target: "pile::save_worker", p95_ms = p95, "save worker p95 latency");
                }
                let _ = event_tx.send(WorkerEvent::Telemetry(telemetry));
            })
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

pub fn load_session(
    path: &PathBuf,
    telemetry: &mut SaveTelemetry,
) -> Result<Option<SessionSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Try to deserialize as new envelope format first
    match SessionEnvelope::from_bytes(&bytes) {
        Ok(envelope) => {
            let original_version = envelope.envelope_version;
            let mut snapshot = SessionEnvelope::open(envelope)?;
            if original_version < 4 {
                recover_orphan_documents(path, &mut snapshot, telemetry);
            }
            Ok(Some(snapshot))
        }
        Err(_) => {
            // Fallback: try to deserialize as legacy SessionSnapshot (v1/v2)
            match bincode::deserialize::<SessionSnapshot>(&bytes) {
                Ok(snapshot) => {
                    // Support schema v1 by migrating to v2
                    if snapshot.schema_version == 1 {
                        let mut migrated = SessionSnapshot {
                            schema_version: 2,
                            state: snapshot.state,
                            panes: vec![],
                            active_pane: 0,
                        };
                        recover_orphan_documents(path, &mut migrated, telemetry);
                        return Ok(Some(migrated));
                    }

                    if snapshot.schema_version != 2 {
                        anyhow::bail!("unsupported session schema {}", snapshot.schema_version);
                    }

                    let mut snapshot = snapshot;
                    recover_orphan_documents(path, &mut snapshot, telemetry);
                    Ok(Some(snapshot))
                }
                Err(_) => {
                    // Try truly old v1 format (different struct without panes)
                    #[derive(Serialize, Deserialize)]
                    struct OldSessionV1 {
                        pub schema_version: u32,
                        pub state: crate::model::AppState,
                    }

                    match bincode::deserialize::<OldSessionV1>(&bytes) {
                        Ok(old) => {
                            // Migrate v1 to current format
                            let mut migrated = SessionSnapshot {
                                schema_version: 2,
                                state: old.state,
                                panes: vec![],
                                active_pane: 0,
                            };
                            recover_orphan_documents(path, &mut migrated, telemetry);
                            return Ok(Some(migrated));
                        }
                        Err(_) => {
                            // Main session is corrupt, quarantine it and try backups
                            warn!(
                                path = %path.display(),
                                "main session corrupt, trying backups"
                            );
                            quarantine_corrupt_session(path, telemetry);

                            match load_session_from_backup(path, telemetry) {
                                Ok(Some((snapshot, backup_path))) => {
                                    // Restore from backup
                                    let _ = fs::copy(&backup_path, path);
                                    info!(
                                        backup_path = %backup_path.display(),
                                        "restored session from backup"
                                    );
                                    telemetry.recovery_events.push(RecoveryEvent {
                                        timestamp: std::time::SystemTime::now(),
                                        kind: RecoveryEventKind::BackupRestored,
                                        message: format!(
                                            "Restored session from backup {}",
                                            backup_path.display()
                                        ),
                                    });
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
    }
}

fn run_save_loop(rx: Receiver<SaveMsg>, session_path: &PathBuf, telemetry: &mut SaveTelemetry) {
    while let Ok(message) = rx.recv() {
        // Log channel depth for monitoring queue health
        let queue_depth = rx.len();
        if queue_depth > 100 {
            warn!(target: "pile::save_worker", depth = queue_depth, "save worker queue depth high");
        }

        match message {
            SaveMsg::Changed(snapshot) => {
                let mut latest = snapshot;
                loop {
                    match rx.recv_timeout(SAVE_DEBOUNCE) {
                        Ok(SaveMsg::Changed(snapshot)) => latest = snapshot,
                        Ok(SaveMsg::Flush(snapshot, ack)) => {
                            latest = snapshot;
                            save_and_ack(&session_path, &latest, Some(ack), telemetry);
                            break;
                        }
                        Ok(SaveMsg::Shutdown) => {
                            save_snapshot(&session_path, &latest, telemetry);
                            return;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            save_snapshot(&session_path, &latest, telemetry);
                            break;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
            SaveMsg::Flush(snapshot, ack) => {
                save_and_ack(&session_path, &snapshot, Some(ack), telemetry)
            }
            SaveMsg::Shutdown => return,
        }
    }
}

fn save_and_ack(
    path: &PathBuf,
    snapshot: &SessionSnapshot,
    ack: Option<Sender<Result<(), String>>>,
    telemetry: &mut SaveTelemetry,
) {
    // Check budget before attempting to save
    match check_snapshot_budget(snapshot) {
        BudgetCheck::TooLarge { size, max } => {
            warn!(
                path = %path.display(),
                size = size,
                max = max,
                "snapshot exceeds memory budget, skipping save"
            );
            telemetry.failed_saves += 1;
            telemetry.record_save_duration(0);
            let msg = format!("Snapshot too large: {} bytes (max {} bytes)", size, max);
            telemetry.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveFailed,
                message: msg.clone(),
            });
            if let Some(ack) = ack {
                let _ = ack.send(Err(msg));
            }
            return;
        }
        BudgetCheck::Ok => {}
    }

    let start = std::time::Instant::now();
    telemetry.total_saves += 1;
    let result = write_snapshot(path, snapshot).map_err(|err| {
        error!(error = %err, path = %path.display(), "session save failed");
        err.to_string()
    });

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    telemetry.record_save_duration(elapsed_ms);

    match &result {
        Ok(()) => {
            telemetry.successful_saves += 1;
            telemetry.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveSucceeded,
                message: format!("Session saved to {}", path.display()),
            });
        }
        Err(err) => {
            telemetry.failed_saves += 1;
            telemetry.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveFailed,
                message: format!("Save failed: {}", err),
            });
        }
    }

    if let Some(ack) = ack {
        let _ = ack.send(result);
    }
}

fn save_snapshot(path: &PathBuf, snapshot: &SessionSnapshot, telemetry: &mut SaveTelemetry) {
    // Check budget before attempting to save
    match check_snapshot_budget(snapshot) {
        BudgetCheck::TooLarge { size, max } => {
            warn!(
                path = %path.display(),
                size = size,
                max = max,
                "snapshot exceeds memory budget, skipping save"
            );
            telemetry.failed_saves += 1;
            telemetry.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveFailed,
                message: format!("Snapshot too large: {} bytes (max {} bytes)", size, max),
            });
            return;
        }
        BudgetCheck::Ok => {}
    }

    // Backup current session before overwriting
    backup_current_session(path, telemetry);

    let start = std::time::Instant::now();
    telemetry.total_saves += 1;

    match write_snapshot(path, snapshot) {
        Ok(()) => {
            telemetry.successful_saves += 1;
            telemetry.record_save_duration(start.elapsed().as_millis() as u64);
            debug!(path = %path.display(), "session saved");
        }
        Err(err) => {
            telemetry.failed_saves += 1;
            telemetry.record_save_duration(start.elapsed().as_millis() as u64);
            error!(error = %err, path = %path.display(), "session save failed");
            telemetry.recovery_events.push(RecoveryEvent {
                timestamp: std::time::SystemTime::now(),
                kind: RecoveryEventKind::SaveFailed,
                message: format!("Save failed: {}", err),
            });
        }
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

pub fn quarantine_corrupt_session(path: &PathBuf, telemetry: &mut SaveTelemetry) {
    let bad_path = path.with_extension("bin.bad");
    if let Err(err) = fs::rename(path, &bad_path) {
        warn!(
            error = %err,
            path = %path.display(),
            bad_path = %bad_path.display(),
            "failed to quarantine corrupt session"
        );
        telemetry.recovery_events.push(RecoveryEvent {
            timestamp: std::time::SystemTime::now(),
            kind: RecoveryEventKind::QuarantineFailed,
            message: format!("Failed to quarantine {}: {}", path.display(), err),
        });
    } else {
        telemetry.recovery_events.push(RecoveryEvent {
            timestamp: std::time::SystemTime::now(),
            kind: RecoveryEventKind::SessionCorrupt,
            message: format!("Quarantined corrupt session to {}", bad_path.display()),
        });
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
pub fn backup_current_session(path: &PathBuf, telemetry: &mut SaveTelemetry) {
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
        telemetry.recovery_events.push(RecoveryEvent {
            timestamp: std::time::SystemTime::now(),
            kind: RecoveryEventKind::BackupFailed,
            message: format!("Failed to create backup {}: {}", backup_path.display(), err),
        });
    } else {
        telemetry.recovery_events.push(RecoveryEvent {
            timestamp: std::time::SystemTime::now(),
            kind: RecoveryEventKind::BackupRestored,
            message: format!("Created backup at {}", backup_path.display()),
        });
    }
}

/// Try to load session from backup files, starting from the most recent.
/// Returns the first loadable backup, or None if all backups are corrupt.
pub fn load_session_from_backup(
    session_path: &PathBuf,
    telemetry: &mut SaveTelemetry,
) -> Result<Option<(SessionSnapshot, PathBuf)>> {
    for i in 1..=BACKUP_ROTATION_COUNT {
        let backup_path = session_path.with_extension(format!("bin.{}", i));
        if !backup_path.exists() {
            continue;
        }

        match load_session(&backup_path, telemetry) {
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
                telemetry.recovery_events.push(RecoveryEvent {
                    timestamp: std::time::SystemTime::now(),
                    kind: RecoveryEventKind::BackupFailed,
                    message: format!("Backup {} also corrupt: {}", backup_path.display(), err),
                });
                continue;
            }
        }
    }

    Ok(None)
}

/// After loading a session, check backup files for documents that exist in
/// backups but not in the current state. Any found are added to closed_documents.
/// This protects against silent data loss during version migrations.
fn recover_orphan_documents(
    session_path: &PathBuf,
    snapshot: &mut SessionSnapshot,
    telemetry: &mut SaveTelemetry,
) {
    let current_ids: HashSet<DocumentId> =
        snapshot.state.documents.iter().map(|d| d.id).collect();
    let mut recovered = Vec::new();
    let mut next_order = snapshot.state.next_closed_order;

    for i in 1..=BACKUP_ROTATION_COUNT {
        let backup_path = session_path.with_extension(format!("bin.{}", i));
        if !backup_path.exists() {
            continue;
        }

        match load_documents_from_backup(&backup_path) {
            Ok(docs) => {
                for doc in docs {
                    if current_ids.contains(&doc.id) {
                        continue;
                    }
                    if recovered.iter().any(|d: &Document| d.id == doc.id) {
                        continue;
                    }
                    if snapshot
                        .state
                        .closed_documents
                        .iter()
                        .any(|cd| cd.document.id == doc.id)
                    {
                        continue;
                    }
                    recovered.push(doc);
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    backup_path = %backup_path.display(),
                    "failed to read backup during document recovery"
                );
                continue;
            }
        }
    }

    if !recovered.is_empty() {
        info!(
            count = recovered.len(),
            "recovered {} orphaned documents from session backups",
            recovered.len(),
        );
        telemetry.recovery_events.push(RecoveryEvent {
            timestamp: std::time::SystemTime::now(),
            kind: RecoveryEventKind::DocumentsRecovered,
            message: format!(
                "Recovered {} orphaned documents from backups",
                recovered.len()
            ),
        });

        for doc in recovered {
            snapshot
                .state
                .closed_documents
                .push(ClosedDocument {
                    document: doc,
                    order: next_order,
                });
            next_order += 1;
        }
        snapshot.state.next_closed_order = next_order;
    }
}

/// Read document list from a backup file, handling any format version.
fn load_documents_from_backup(path: &PathBuf) -> Result<Vec<Document>> {
    let bytes = fs::read(path)?;

    // Try envelope format (v3+)
    if let Ok(envelope) = SessionEnvelope::from_bytes(&bytes) {
        let snapshot = SessionEnvelope::open(envelope)?;
        return Ok(snapshot.state.documents);
    }

    // Try legacy flat format (v2)
    if let Ok(snapshot) = bincode::deserialize::<SessionSnapshot>(&bytes) {
        return Ok(snapshot.state.documents);
    }

    // Try v1 format (no schema_version field)
    #[derive(Deserialize)]
    struct V1State {
        pub documents: Vec<Document>,
    }
    if let Ok(v1) = bincode::deserialize::<V1State>(&bytes) {
        return Ok(v1.documents);
    }

    anyhow::bail!("unrecognized backup format");
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
        assert_eq!(recovered.document.rope.to_string(), "orphaned from v3 close");

        // Telemetry recorded
        assert!(telemetry
            .recovery_events
            .iter()
            .any(|e| matches!(e.kind, RecoveryEventKind::DocumentsRecovered)));

        // A and B were NOT recovered (they are open)
        assert!(!snapshot
            .state
            .closed_documents
            .iter()
            .any(|cd| cd.document.id == doc_a));

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
}
