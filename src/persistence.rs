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
use tracing::{debug, error, warn};

use serde::Deserialize;

use crate::model::{SessionSnapshot, PaneSnapshot};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const SESSION_FILE: &str = ".session.bin";

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

pub fn load_session(path: &PathBuf) -> Result<Option<SessionSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Try to deserialize as version 2 first
    match bincode::deserialize::<SessionSnapshot>(&bytes) {
        Ok(snapshot) => {
            if snapshot.schema_version == 2 {
                Ok(Some(snapshot))
            } else {
                anyhow::bail!("unsupported session schema {}", snapshot.schema_version);
            }
        }
        Err(_) => {
            // Try to deserialize as version 1 and migrate
            #[derive(Deserialize)]
            struct SessionSnapshotV1 {
                schema_version: u32,
                state: crate::model::AppState,
            }

            let snapshot_v1: SessionSnapshotV1 = bincode::deserialize(&bytes)
                .with_context(|| format!("failed to decode {}", path.display()))?;

            if snapshot_v1.schema_version != 1 {
                anyhow::bail!("unsupported session schema {}", snapshot_v1.schema_version);
            }

            // Migrate to version 2
            Ok(Some(SessionSnapshot {
                schema_version: 2,
                state: snapshot_v1.state,
                panes: vec![PaneSnapshot {
                    document_id: snapshot_v1.state.active_document,
                    preferred_column: None,
                    visible_rows: None,
                    column_selection: false,
                    column_selection_anchor_col: None,
                }],
                active_pane: 0,
            }))
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

    let bytes = bincode::serialize(snapshot)?;
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
        assert_eq!(loaded.schema_version, snapshot.schema_version);
        assert_eq!(loaded.state.documents.len(), 1);

        worker.shutdown();
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(dir);
    }
}
