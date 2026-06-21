use std::{
    error::Error,
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    thread::{self, JoinHandle},
    time::SystemTime,
};

use anyhow::{Context, Result, anyhow};
use crossbeam_channel::{Receiver, Sender, bounded};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/nikaspran/pile/releases/download/continuous/pile-update-manifest.json";
const STAGED_METADATA_FILE: &str = "staged-update.json";
const APPLY_SCRIPT: &str = "apply-staged-update.sh";

#[derive(Debug)]
struct ManifestNotPublished;

impl fmt::Display for ManifestNotPublished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no continuous update manifest has been published yet")
    }
}

impl Error for ManifestNotPublished {}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: String,
    pub tag: String,
    pub commit: String,
    pub minimum_session_schema: u32,
    pub artifacts: Vec<UpdateArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateArtifact {
    pub name: String,
    pub platform: String,
    pub kind: String,
    pub sha256: String,
    pub url: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct StagedUpdate {
    pub version: String,
    pub channel: String,
    pub tag: String,
    pub commit: String,
    pub artifact_name: String,
    pub target: String,
    pub app_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct UpdateUiState {
    pub checking: bool,
    pub staged: Option<StagedUpdate>,
    pub last_checked: Option<SystemTime>,
    pub last_error: Option<String>,
    pub not_applicable: Option<String>,
}

impl UpdateUiState {
    pub fn load() -> Self {
        let staged = staged_update()
            .ok()
            .flatten()
            .filter(|update| update.commit != current_build_commit());
        Self {
            checking: false,
            staged,
            last_checked: None,
            last_error: None,
            not_applicable: None,
        }
    }

    pub fn menu_state(&self) -> crate::native_menu::NativeUpdateMenuState {
        let label = if self.checking {
            "Checking for Updates..."
        } else if let Some(update) = &self.staged {
            return crate::native_menu::NativeUpdateMenuState {
                label: format!("Restart to Update ({})", short_commit(&update.commit)),
                enabled: true,
            };
        } else if let Some(error) = &self.last_error {
            if error.trim().is_empty() {
                "Check for Updates..."
            } else {
                "Check for Updates Failed"
            }
        } else if let Some(reason) = &self.not_applicable {
            if reason.contains("published") {
                "No Published Updates"
            } else {
                "No Applicable Update"
            }
        } else if self.last_checked.is_some() {
            "No Updates Available"
        } else {
            "Check for Updates..."
        };

        crate::native_menu::NativeUpdateMenuState {
            label: label.to_owned(),
            enabled: !self.checking,
        }
    }

    pub fn apply_event(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::Checking => {
                self.checking = true;
                self.last_error = None;
                self.not_applicable = None;
            }
            UpdateEvent::UpToDate => {
                self.checking = false;
                self.last_checked = Some(SystemTime::now());
                self.last_error = None;
                self.not_applicable = None;
            }
            UpdateEvent::Staged { update } => {
                self.checking = false;
                self.last_checked = Some(SystemTime::now());
                self.last_error = None;
                self.not_applicable = None;
                self.staged = Some(update);
            }
            UpdateEvent::NotApplicable { reason } => {
                self.checking = false;
                self.last_checked = Some(SystemTime::now());
                self.last_error = None;
                self.not_applicable = Some(reason);
            }
            UpdateEvent::Failed { message } => {
                self.checking = false;
                self.last_checked = Some(SystemTime::now());
                self.last_error = Some(message);
            }
        }
    }
}

#[derive(Debug)]
pub enum UpdateRequest {
    Check,
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Checking,
    UpToDate,
    Staged { update: StagedUpdate },
    NotApplicable { reason: String },
    Failed { message: String },
}

pub struct UpdateWorker {
    tx: Sender<UpdateRequest>,
    rx: Receiver<UpdateEvent>,
    handle: Option<JoinHandle<()>>,
}

impl UpdateWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = bounded(16);
        let (event_tx, event_rx) = bounded(16);
        let handle = thread::Builder::new()
            .name("pile-update-worker".to_owned())
            .spawn(move || update_worker_loop(request_rx, event_tx))
            .expect("failed to spawn update worker");

        Self {
            tx: request_tx,
            rx: event_rx,
            handle: Some(handle),
        }
    }

    pub fn request_check(&self) {
        let _ = self.tx.send(UpdateRequest::Check);
    }

    pub fn try_recv(&self) -> Option<UpdateEvent> {
        self.rx.try_recv().ok()
    }
}

impl Drop for UpdateWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(UpdateRequest::Shutdown);
        let _ = self.handle.take();
    }
}

fn update_worker_loop(request_rx: Receiver<UpdateRequest>, event_tx: Sender<UpdateEvent>) {
    while let Ok(request) = request_rx.recv() {
        match request {
            UpdateRequest::Check => {
                let _ = event_tx.send(UpdateEvent::Checking);
                let event = match check_and_stage_update(DEFAULT_MANIFEST_URL) {
                    Ok(event) => event,
                    Err(err) => UpdateEvent::Failed {
                        message: err.to_string(),
                    },
                };
                let _ = event_tx.send(event);
            }
            UpdateRequest::Shutdown => break,
        }
    }
}

pub fn current_build_commit() -> &'static str {
    env!("PILE_BUILD_COMMIT")
}

pub fn current_target() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    {
        "unknown"
    }
}

pub fn current_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown"
    }
}

pub fn select_current_artifact(manifest: &UpdateManifest) -> Option<&UpdateArtifact> {
    let platform = current_platform();
    let target = current_target();
    manifest.artifacts.iter().find(|artifact| {
        artifact.platform == platform
            && artifact.target.as_deref().map_or_else(
                || artifact.name.contains(target),
                |artifact_target| artifact_target == target,
            )
    })
}

pub fn check_and_stage_update(manifest_url: &str) -> Result<UpdateEvent> {
    let manifest = match fetch_manifest(manifest_url) {
        Ok(manifest) => manifest,
        Err(err) if err.downcast_ref::<ManifestNotPublished>().is_some() => {
            return Ok(UpdateEvent::NotApplicable {
                reason: "no continuous update manifest has been published yet".to_owned(),
            });
        }
        Err(err) => return Err(err),
    };
    if manifest.minimum_session_schema > crate::persistence::SESSION_SCHEMA_VERSION {
        return Ok(UpdateEvent::NotApplicable {
            reason: format!(
                "release requires session schema {}, current app supports {}",
                manifest.minimum_session_schema,
                crate::persistence::SESSION_SCHEMA_VERSION
            ),
        });
    }

    if manifest.commit == current_build_commit() {
        return Ok(UpdateEvent::UpToDate);
    }

    let Some(artifact) = select_current_artifact(&manifest) else {
        return Ok(UpdateEvent::NotApplicable {
            reason: format!(
                "no {} artifact for {}",
                current_platform(),
                current_target()
            ),
        });
    };

    if !can_stage_update() {
        return Ok(UpdateEvent::NotApplicable {
            reason: "current install is not a writable macOS .app bundle".to_owned(),
        });
    }

    let staged = stage_artifact(&manifest, artifact)?;
    Ok(UpdateEvent::Staged { update: staged })
}

pub fn fetch_manifest(url: &str) -> Result<UpdateManifest> {
    let text = if let Some(path) = url.strip_prefix("file://") {
        fs::read_to_string(path).with_context(|| format!("failed to read manifest {path}"))?
    } else {
        match ureq::get(url).call() {
            Ok(response) => response
                .into_string()
                .context("failed to read update manifest response")?,
            Err(ureq::Error::Status(404, _)) => return Err(anyhow!(ManifestNotPublished)),
            Err(err) => return Err(anyhow!("failed to fetch update manifest: {err}")),
        }
    };

    serde_json::from_str(&text).context("failed to parse update manifest")
}

pub fn staged_update() -> Result<Option<StagedUpdate>> {
    let path = staged_metadata_path();
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path)
        .with_context(|| format!("failed to read staged update {}", path.display()))?;
    let update = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse staged update {}", path.display()))?;
    Ok(Some(update))
}

pub fn restart_to_update() -> Result<()> {
    let Some(update) = staged_update()? else {
        return Err(anyhow!("no staged update is ready"));
    };
    spawn_apply_helper(&update).context("failed to start update relaunch helper")
}

pub fn apply_staged_update_on_launch() -> Result<bool> {
    let Some(update) = staged_update()? else {
        return Ok(false);
    };
    if update.commit == current_build_commit() {
        let _ = fs::remove_file(staged_metadata_path());
        return Ok(false);
    }
    if current_app_bundle().is_none() || !update.app_path.is_dir() {
        return Ok(false);
    }

    spawn_apply_helper(&update).context("failed to start staged update helper")?;
    Ok(true)
}

fn stage_artifact(manifest: &UpdateManifest, artifact: &UpdateArtifact) -> Result<StagedUpdate> {
    let root = update_root_dir();
    let stage_dir = root.join("staged").join(&manifest.commit);
    let download_path = stage_dir.join(&artifact.name);
    let extract_dir = stage_dir.join("extracted");

    let _ = fs::remove_dir_all(&stage_dir);
    fs::create_dir_all(&extract_dir)
        .with_context(|| format!("failed to create update stage {}", extract_dir.display()))?;

    download_verified(&artifact.url, &download_path, &artifact.sha256)?;
    let app_path = extract_macos_app(&download_path, &extract_dir)?;

    let staged = StagedUpdate {
        version: manifest.version.clone(),
        channel: manifest.channel.clone(),
        tag: manifest.tag.clone(),
        commit: manifest.commit.clone(),
        artifact_name: artifact.name.clone(),
        target: artifact
            .target
            .clone()
            .unwrap_or_else(|| current_target().to_owned()),
        app_path,
    };

    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    fs::write(staged_metadata_path(), serde_json::to_vec_pretty(&staged)?)
        .context("failed to write staged update metadata")?;

    Ok(staged)
}

fn download_verified(url: &str, path: &Path, expected_sha256: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut file = fs::File::create(path)
        .with_context(|| format!("failed to create update download {}", path.display()))?;

    if let Some(source) = url.strip_prefix("file://") {
        let mut source = fs::File::open(source)
            .with_context(|| format!("failed to open update artifact {source}"))?;
        copy_and_hash(&mut source, &mut file, &mut hasher)?;
    } else {
        let response = ureq::get(url)
            .call()
            .map_err(|err| anyhow!("failed to download update artifact: {err}"))?;
        let mut reader = response.into_reader();
        copy_and_hash(&mut reader, &mut file, &mut hasher)?;
    }

    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = fs::remove_file(path);
        return Err(anyhow!(
            "update artifact hash mismatch: expected {expected_sha256}, got {actual}"
        ));
    }
    Ok(())
}

fn copy_and_hash<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    hasher: &mut Sha256,
) -> io::Result<u64> {
    let mut total = 0;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        writer.write_all(&buf[..read])?;
        total += read as u64;
    }
    Ok(total)
}

fn can_stage_update() -> bool {
    if current_platform() != "macos" {
        return false;
    }
    let Some(bundle) = current_app_bundle() else {
        return false;
    };
    bundle
        .parent()
        .and_then(|parent| fs::metadata(parent).ok())
        .is_some_and(|metadata| !metadata.permissions().readonly())
}

fn extract_macos_app(zip_path: &Path, extract_dir: &Path) -> Result<PathBuf> {
    if current_platform() != "macos" {
        return Err(anyhow!("automatic apply is only implemented for macOS"));
    }

    let status = Command::new("/usr/bin/ditto")
        .arg("-x")
        .arg("-k")
        .arg(zip_path)
        .arg(extract_dir)
        .status()
        .context("failed to launch ditto to extract update")?;
    if !status.success() {
        return Err(anyhow!("ditto failed to extract update artifact"));
    }

    let expected = extract_dir.join("pile.app");
    if expected.is_dir() {
        return Ok(expected);
    }

    for entry in fs::read_dir(extract_dir)
        .with_context(|| format!("failed to read {}", extract_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "app") && path.is_dir() {
            return Ok(path);
        }
    }

    Err(anyhow!("update artifact did not contain a .app bundle"))
}

fn spawn_apply_helper(update: &StagedUpdate) -> Result<()> {
    let Some(current_app) = current_app_bundle() else {
        return Err(anyhow!("current executable is not inside a .app bundle"));
    };
    if !update.app_path.is_dir() {
        return Err(anyhow!("staged app bundle is missing"));
    }

    let root = update_root_dir();
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    let script_path = root.join(APPLY_SCRIPT);
    let backup_app = current_app.with_extension("app.old");
    let failure_path = root.join("last-update-failure.txt");
    let metadata_path = staged_metadata_path();
    let stage_parent = update
        .app_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&root)
        .to_path_buf();

    let script = format!(
        r#"#!/bin/sh
set -eu
old_app={old_app}
new_app={new_app}
backup_app={backup_app}
metadata_path={metadata_path}
stage_parent={stage_parent}
failure_path={failure_path}
pid={pid}

while kill -0 "$pid" 2>/dev/null; do
  sleep 0.2
done

rm -rf "$backup_app"
if [ -d "$old_app" ]; then
  mv "$old_app" "$backup_app"
fi

if /usr/bin/ditto "$new_app" "$old_app"; then
  rm -rf "$backup_app"
  rm -f "$metadata_path"
  rm -rf "$stage_parent"
  /usr/bin/open "$old_app"
else
  rm -rf "$old_app"
  if [ -d "$backup_app" ]; then
    mv "$backup_app" "$old_app"
  fi
  echo "failed to replace app bundle" > "$failure_path"
  rm -f "$metadata_path"
  /usr/bin/open "$old_app"
  exit 1
fi
"#,
        old_app = shell_quote(&current_app),
        new_app = shell_quote(&update.app_path),
        backup_app = shell_quote(&backup_app),
        metadata_path = shell_quote(&metadata_path),
        stage_parent = shell_quote(&stage_parent),
        failure_path = shell_quote(&failure_path),
        pid = std::process::id(),
    );

    fs::write(&script_path, script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;

    Command::new("/bin/sh")
        .arg(&script_path)
        .spawn()
        .context("failed to spawn update helper")?;

    Ok(())
}

fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(Path::to_path_buf)
}

fn update_root_dir() -> PathBuf {
    crate::persistence::default_settings_path()
        .parent()
        .map_or_else(|| PathBuf::from("updates"), |parent| parent.join("updates"))
}

fn staged_metadata_path() -> PathBuf {
    update_root_dir().join(STAGED_METADATA_FILE)
}

fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_artifact_for_current_target() {
        let manifest = UpdateManifest {
            version: "0.1.0".to_owned(),
            channel: "continuous".to_owned(),
            tag: "continuous".to_owned(),
            commit: "abc".to_owned(),
            minimum_session_schema: crate::persistence::SESSION_SCHEMA_VERSION,
            artifacts: vec![
                artifact("macos", "x86_64-apple-darwin"),
                artifact("macos", "aarch64-apple-darwin"),
                artifact("linux", "x86_64-unknown-linux-gnu"),
            ],
        };

        let selected = select_current_artifact(&manifest).unwrap();
        assert_eq!(selected.platform, current_platform());
        assert_eq!(selected.target.as_deref(), Some(current_target()));
    }

    #[test]
    fn rejects_current_commit_as_up_to_date() {
        let manifest = UpdateManifest {
            version: "0.1.0".to_owned(),
            channel: "continuous".to_owned(),
            tag: "continuous".to_owned(),
            commit: current_build_commit().to_owned(),
            minimum_session_schema: crate::persistence::SESSION_SCHEMA_VERSION,
            artifacts: vec![artifact(current_platform(), current_target())],
        };

        assert_eq!(manifest.commit, current_build_commit());
    }

    #[test]
    fn menu_state_replaces_check_with_restart_when_staged() {
        let mut state = UpdateUiState {
            checking: false,
            staged: None,
            last_checked: None,
            last_error: None,
            not_applicable: None,
        };
        assert_eq!(state.menu_state().label, "Check for Updates...");
        assert!(state.menu_state().enabled);

        state.staged = Some(StagedUpdate {
            version: "0.1.0".to_owned(),
            channel: "continuous".to_owned(),
            tag: "continuous".to_owned(),
            commit: "abcdef123".to_owned(),
            artifact_name: "pile.zip".to_owned(),
            target: current_target().to_owned(),
            app_path: PathBuf::from("pile.app"),
        });

        let menu = state.menu_state();
        assert!(menu.enabled);
        assert!(menu.label.contains("Restart to Update"));
        assert!(menu.label.contains("abcdef1"));
    }

    #[test]
    fn menu_state_reports_non_error_update_statuses() {
        let mut state = UpdateUiState {
            checking: false,
            staged: None,
            last_checked: Some(SystemTime::now()),
            last_error: None,
            not_applicable: None,
        };
        assert_eq!(state.menu_state().label, "No Updates Available");

        state.not_applicable = Some("no continuous update manifest has been published yet".into());
        assert_eq!(state.menu_state().label, "No Published Updates");
    }

    fn artifact(platform: &str, target: &str) -> UpdateArtifact {
        UpdateArtifact {
            name: format!("pile-0.1.0-{target}-{platform}.zip"),
            platform: platform.to_owned(),
            kind: "zip".to_owned(),
            sha256: "0".repeat(64),
            url: "file:///tmp/pile.zip".to_owned(),
            target: Some(target.to_owned()),
        }
    }
}
