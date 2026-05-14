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
    FileOperationFailed,
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
                run_save_loop(rx, &session_path, &mut telemetry, None)
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
                run_save_loop(rx, &session_path, &mut telemetry, Some(&event_tx));
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

fn run_save_loop(
    rx: Receiver<SaveMsg>,
    session_path: &PathBuf,
    telemetry: &mut SaveTelemetry,
    event_tx: Option<&Sender<WorkerEvent>>,
) {
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
                            save_and_ack(&session_path, &latest, Some(ack), telemetry, event_tx);
                            break;
                        }
                        Ok(SaveMsg::Shutdown) => {
                            save_snapshot(&session_path, &latest, telemetry, event_tx);
                            return;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            save_snapshot(&session_path, &latest, telemetry, event_tx);
                            break;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
            SaveMsg::Flush(snapshot, ack) => {
                save_and_ack(&session_path, &snapshot, Some(ack), telemetry, event_tx)
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
    event_tx: Option<&Sender<WorkerEvent>>,
) {
    let event_start = telemetry.recovery_events.len();

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
            emit_worker_events(event_tx, telemetry, event_start);
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
    emit_worker_events(event_tx, telemetry, event_start);
}

fn save_snapshot(
    path: &PathBuf,
    snapshot: &SessionSnapshot,
    telemetry: &mut SaveTelemetry,
    event_tx: Option<&Sender<WorkerEvent>>,
) {
    let event_start = telemetry.recovery_events.len();

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
            emit_worker_events(event_tx, telemetry, event_start);
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

    emit_worker_events(event_tx, telemetry, event_start);
}

fn emit_worker_events(
    event_tx: Option<&Sender<WorkerEvent>>,
    telemetry: &SaveTelemetry,
    event_start: usize,
) {
    let Some(event_tx) = event_tx else {
        return;
    };

    for event in telemetry.recovery_events.iter().skip(event_start) {
        match event.kind {
            RecoveryEventKind::SaveFailed
            | RecoveryEventKind::FileOperationFailed
            | RecoveryEventKind::BackupFailed
            | RecoveryEventKind::QuarantineFailed
            | RecoveryEventKind::SessionCorrupt
            | RecoveryEventKind::DocumentsRecovered => {
                let _ = event_tx.send(WorkerEvent::Recovery(event.clone()));
            }
            RecoveryEventKind::BackupRestored | RecoveryEventKind::SaveSucceeded => {}
        }
    }

    let _ = event_tx.send(WorkerEvent::Telemetry(telemetry.clone()));
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
    let current_ids: HashSet<DocumentId> = snapshot.state.documents.iter().map(|d| d.id).collect();
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
            snapshot.state.closed_documents.push(ClosedDocument {
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
mod tests;
