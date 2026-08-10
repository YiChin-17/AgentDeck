//! Whether the configured Library is the one AgentDeck expects, and reachable.
//!
//! A configured Library may live on an external volume that is not mounted. The
//! probe here answers "can this session safely touch the Library?" without ever
//! creating the path — creating an absent mountpoint is what turns "the volume
//! is unplugged" into "the library is empty", which downstream reindex and sync
//! would then read as a deletion.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::central_repo;
use super::error::AppError;

/// AgentDeck-owned identity file written at the Library root.
pub const MARKER_FILE_NAME: &str = ".agentdeck-library.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LibraryMarker {
    id: String,
    created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryState {
    Online,
    Offline,
}

/// Stable reason codes. The frontend maps these to localized text, so the
/// wire values must not change once shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryReason {
    Ok,
    /// The configured path is not there — typically an unmounted volume.
    MissingPath,
    /// The path exists but cannot be listed (permissions, or not a directory).
    NotReadable,
    /// Readable, but AgentDeck may not write to it.
    NotWritable,
    /// The path holds a different Library than the one that was configured.
    IdentityMismatch,
    /// Reachable, but reloading Library metadata failed, so the app's view of
    /// it would be wrong.
    RefreshFailed,
    /// Reachable and refreshed, but file watching could not be restarted, so
    /// external edits would go unnoticed.
    WatcherFailed,
}

impl LibraryReason {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryReason::Ok => "ok",
            LibraryReason::MissingPath => "missing_path",
            LibraryReason::NotReadable => "not_readable",
            LibraryReason::NotWritable => "not_writable",
            LibraryReason::IdentityMismatch => "identity_mismatch",
            LibraryReason::RefreshFailed => "refresh_failed",
            LibraryReason::WatcherFailed => "watcher_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryProbe {
    pub state: LibraryState,
    pub reason: LibraryReason,
    /// Identity found at the root, when there is one. `None` means the Library
    /// has not been adopted yet.
    pub library_id: Option<String>,
}

impl LibraryProbe {
    fn offline(reason: LibraryReason) -> Self {
        Self {
            state: LibraryState::Offline,
            reason,
            library_id: None,
        }
    }

    pub fn is_online(&self) -> bool {
        self.state == LibraryState::Online
    }
}

fn read_marker(root: &Path) -> Option<LibraryMarker> {
    let raw = fs::read_to_string(root.join(MARKER_FILE_NAME)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// The identity the Library at `root` currently advertises, if any.
pub fn read_library_id(root: &Path) -> Option<String> {
    read_marker(root).map(|marker| marker.id)
}

/// Whether the directory's permission bits allow this process to write.
///
// SIMPLE: permission bits only. A read-only mount still reports writable here;
// that case surfaces when the first real write fails and the runtime error path
// flips the state to offline. Probing by writing a file would violate the
// no-side-effects contract this whole module exists to keep.
fn is_writable(metadata: &fs::Metadata) -> bool {
    !metadata.permissions().readonly()
}

/// Inspect `root` without modifying anything on disk.
///
/// `expected_id` is the identity recorded in the config. When it is `None` the
/// Library has not been adopted yet and any readable directory qualifies, so a
/// legacy layout can be adopted on its first online launch.
pub fn probe_library(root: &Path, expected_id: Option<&str>) -> LibraryProbe {
    let metadata = match fs::metadata(root) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return LibraryProbe::offline(LibraryReason::MissingPath);
        }
        Err(_) => return LibraryProbe::offline(LibraryReason::NotReadable),
    };
    if !metadata.is_dir() || fs::read_dir(root).is_err() {
        return LibraryProbe::offline(LibraryReason::NotReadable);
    }

    let found_id = read_marker(root).map(|marker| marker.id);
    if let Some(expected) = expected_id {
        // A missing marker on a configured Library means this is not the volume
        // that was adopted — the same mountpoint can hold a different disk.
        if found_id.as_deref() != Some(expected) {
            return LibraryProbe::offline(LibraryReason::IdentityMismatch);
        }
    }

    if !is_writable(&metadata) {
        return LibraryProbe::offline(LibraryReason::NotWritable);
    }

    LibraryProbe {
        state: LibraryState::Online,
        reason: LibraryReason::Ok,
        library_id: found_id,
    }
}

/// Write the identity marker at `root`, returning the identity to record in the
/// config. Existing markers are kept as-is so adoption stays idempotent.
///
/// Unlike [`probe_library`], this writes — call it only for a root that has
/// already probed online.
pub fn adopt_library(root: &Path) -> Result<String> {
    if let Some(marker) = read_marker(root) {
        return Ok(marker.id);
    }
    let marker = LibraryMarker {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now().timestamp(),
    };
    let path = root.join(MARKER_FILE_NAME);
    fs::write(&path, serde_json::to_vec_pretty(&marker)?)
        .with_context(|| format!("cannot write Library marker at {}", path.display()))?;
    Ok(marker.id)
}

// ── runtime availability state ────────────────────────────────────────────
//
// Every flow that can write to the Library reads this one state, so a single
// probe decides for all of them. Per-command path checks were the alternative
// and they leak: a direct IPC call or a background job skips whatever the UI
// happens to disable.

/// Availability as the running app currently understands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryAvailability {
    pub state: LibraryState,
    pub reason: LibraryReason,
    /// The Library root this verdict is about.
    pub configured_path: PathBuf,
    pub library_id: Option<String>,
}

/// Wire shape handed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryAvailabilityDto {
    pub state: LibraryState,
    /// Stable reason code, never a localized or free-form string.
    pub reason: &'static str,
    pub configured_path: String,
    pub library_id: Option<String>,
}

impl LibraryAvailability {
    pub fn to_dto(&self) -> LibraryAvailabilityDto {
        LibraryAvailabilityDto {
            state: self.state,
            reason: self.reason.as_str(),
            configured_path: self.configured_path.to_string_lossy().to_string(),
            library_id: self.library_id.clone(),
        }
    }

    pub fn is_online(&self) -> bool {
        self.state == LibraryState::Online
    }
}

static AVAILABILITY: OnceLock<RwLock<LibraryAvailability>> = OnceLock::new();

fn cell() -> &'static RwLock<LibraryAvailability> {
    // First read probes rather than assuming online: an unknown Library must
    // never be treated as usable.
    AVAILABILITY.get_or_init(|| RwLock::new(probe_configured_library()))
}

/// Probe whatever the config currently points at, without publishing it.
fn probe_configured_library() -> LibraryAvailability {
    let root = central_repo::library_base_dir();
    // A root named explicitly on the CLI is not the app's configured Library,
    // so the stored identity does not apply to it.
    let expected = if central_repo::base_dir_override_active() {
        None
    } else {
        central_repo::configured_library_id()
    };
    let probe = probe_library(&root, expected.as_deref());
    LibraryAvailability {
        state: probe.state,
        reason: probe.reason,
        configured_path: root,
        library_id: probe.library_id,
    }
}

pub fn availability() -> LibraryAvailability {
    cell().read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn availability_dto() -> LibraryAvailabilityDto {
    availability().to_dto()
}

pub fn set_availability(next: LibraryAvailability) {
    *cell().write().unwrap_or_else(|e| e.into_inner()) = next;
}

/// Publish a verdict and align file watching with it.
///
/// Watching is paused for the whole time the Library is not verified: an
/// unmounted volume emits a removal event for every file it held, and acting on
/// those is indistinguishable from the user deleting the library.
fn publish(next: LibraryAvailability) {
    let online = next.is_online();
    set_availability(next);
    if online {
        super::file_watcher::resume_watching();
    } else {
        super::file_watcher::pause_watching();
    }
}

/// Re-probe the configured Library and publish the verdict. An online Library
/// that carries no identity yet is adopted here, which is how a legacy or newly
/// chosen root gets its marker.
pub fn refresh_availability() -> LibraryAvailability {
    let mut next = probe_configured_library();
    if next.is_online() && next.library_id.is_none() {
        match adopt_library(&next.configured_path) {
            Ok(id) => {
                // Never write a CLI-supplied root's identity into the app's
                // config: that root belongs to the caller, not to the Library.
                if !central_repo::base_dir_override_active() {
                    if let Err(err) = central_repo::record_library_identity(&id) {
                        log::warn!("library: adopted {id} but could not persist it ({err:#})");
                    }
                }
                next.library_id = Some(id);
            }
            Err(err) => {
                log::warn!("library: cannot adopt {:?} ({err:#})", next.configured_path);
                next.state = LibraryState::Offline;
                next.reason = LibraryReason::NotWritable;
            }
        }
    }
    publish(next.clone());
    next
}

/// Re-verify a Library believed to be online and publish the verdict if it has
/// become unusable. Used by the UI to notice a pulled volume between mutations:
/// nothing else observes an unmount, because watching a vanished root stops
/// producing events.
///
/// Deliberately one-way. An offline verdict is returned untouched, never
/// re-probed, so reconnecting stays an explicit Retry — a poll must not put the
/// app back online behind the user's back.
pub fn poll_availability() -> LibraryAvailability {
    let current = availability();
    if !current.is_online() {
        return current;
    }
    let fresh = probe_configured_library();
    if fresh.is_online() {
        return current;
    }
    publish(fresh.clone());
    fresh
}

/// Force the offline verdict, for tests that need a specific reason without a
/// real volume to probe.
///
/// Production code never declares offline unconditionally: it re-probes
/// (`ensure_library_online`, `poll_availability`) so an unrelated failure —
/// permissions, a full disk — is not reported to the user as a disconnected
/// Library.
#[cfg(test)]
pub fn mark_offline(reason: LibraryReason) {
    let mut next = availability();
    next.state = LibraryState::Offline;
    next.reason = reason;
    next.library_id = None;
    publish(next);
}

/// Work that must succeed before a reconnected Library counts as usable.
/// Injected so the ordering and the failure handling can be tested without a
/// Tauri runtime.
pub trait ReconnectSteps {
    /// Reload the app's view of Library contents.
    fn refresh_metadata(&self) -> Result<()>;
    /// Resume watching the Library for external edits.
    fn restart_watcher(&self) -> Result<()>;
}

/// Re-verify `root` and, only if every reconnect step succeeds, publish online.
///
/// A partially reconnected Library is worse than an offline one: actions would
/// be enabled against a view the app has not refreshed. Any failure therefore
/// leaves the state offline with the reason that stopped it.
pub fn retry_library(
    root: &Path,
    expected_id: Option<&str>,
    steps: &dyn ReconnectSteps,
) -> LibraryAvailability {
    let probe = probe_library(root, expected_id);
    let mut next = LibraryAvailability {
        state: probe.state,
        reason: probe.reason,
        configured_path: root.to_path_buf(),
        library_id: probe.library_id,
    };

    // A Retry never adopts: the identity must already match, or this is a
    // different volume sitting at a familiar path.
    if !next.is_online() {
        publish(next.clone());
        return next;
    }

    let failure = if let Err(err) = steps.refresh_metadata() {
        log::warn!("library: retry failed to refresh metadata ({err:#})");
        Some(LibraryReason::RefreshFailed)
    } else if let Err(err) = steps.restart_watcher() {
        log::warn!("library: retry failed to restart the watcher ({err:#})");
        Some(LibraryReason::WatcherFailed)
    } else {
        None
    };

    // A half-reconnected Library stays offline: the app's view of it was never
    // refreshed, so events would be read against stale state.
    if let Some(reason) = failure {
        next.state = LibraryState::Offline;
        next.reason = reason;
    }
    publish(next.clone());
    next
}

/// Gate for every operation that may write to the Library or to deployment
/// targets. Callers must invoke this before doing any work, not after.
pub fn ensure_library_online() -> std::result::Result<(), AppError> {
    let current = availability();
    // Re-probe a Library believed to be online: the volume may have been pulled
    // since the last check, and this is the last moment before the write. An
    // offline verdict is not re-probed — only an explicit Retry clears it, so a
    // reconnect never happens silently in the middle of an operation.
    let current = if current.is_online() {
        let fresh = probe_configured_library();
        if !fresh.is_online() {
            publish(fresh.clone());
        }
        fresh
    } else {
        current
    };

    if current.is_online() {
        return Ok(());
    }
    Err(AppError::library_offline(current.reason.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use walkdir::WalkDir;

    /// Content fingerprint of a directory tree, used to prove a probe wrote
    /// nothing.
    fn tree_hash(root: &Path) -> Vec<u8> {
        let mut entries: Vec<(String, Vec<u8>)> = WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                let rel = e
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                (rel, fs::read(e.path()).unwrap_or_default())
            })
            .collect();
        entries.sort();
        let mut hasher = Sha256::new();
        for (rel, bytes) in entries {
            hasher.update(rel.as_bytes());
            hasher.update(&bytes);
        }
        hasher.finalize().to_vec()
    }

    fn adopted_library() -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("skills")).unwrap();
        let id = adopt_library(root.path()).unwrap();
        (root, id)
    }

    #[test]
    fn missing_path_is_offline_and_is_not_created() {
        let parent = tempfile::tempdir().unwrap();
        let absent: PathBuf = parent.path().join("Volumes/Ext/Library");

        let probe = probe_library(&absent, Some("some-id"));

        assert_eq!(probe.state, LibraryState::Offline);
        assert_eq!(probe.reason, LibraryReason::MissingPath);
        assert_eq!(probe.reason.as_str(), "missing_path");
        assert!(!absent.exists(), "probing must never create the path");
    }

    #[test]
    fn adopted_library_probes_online_without_touching_the_tree() {
        let (root, id) = adopted_library();
        let before = tree_hash(root.path());

        let probe = probe_library(root.path(), Some(&id));

        assert!(probe.is_online());
        assert_eq!(probe.reason, LibraryReason::Ok);
        assert_eq!(probe.library_id.as_deref(), Some(id.as_str()));
        assert_eq!(tree_hash(root.path()), before, "probe must not write");
    }

    #[test]
    fn same_path_with_a_different_identity_is_offline() {
        let (root, _) = adopted_library();
        let before = tree_hash(root.path());

        let probe = probe_library(root.path(), Some("an-identity-from-another-volume"));

        assert_eq!(probe.state, LibraryState::Offline);
        assert_eq!(probe.reason, LibraryReason::IdentityMismatch);
        assert_eq!(probe.reason.as_str(), "identity_mismatch");
        assert_eq!(
            tree_hash(root.path()),
            before,
            "a mismatched directory must not be adopted or modified"
        );
    }

    #[test]
    fn an_unmarked_directory_at_a_configured_path_is_a_mismatch() {
        // Same mountpoint, different disk: the folder is there but it is not the
        // Library that was configured.
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("skills")).unwrap();

        let probe = probe_library(root.path(), Some("previously-adopted-id"));

        assert_eq!(probe.reason, LibraryReason::IdentityMismatch);
        assert!(!root.path().join(MARKER_FILE_NAME).exists());
    }

    #[test]
    fn an_unadopted_library_probes_online_for_first_time_adoption() {
        // A legacy layout has no marker yet; with no expected identity it may be
        // adopted on this launch.
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("skills")).unwrap();

        let probe = probe_library(root.path(), None);

        assert!(probe.is_online());
        assert_eq!(probe.library_id, None);
    }

    #[test]
    #[cfg(unix)]
    fn a_read_only_library_is_offline() {
        use std::os::unix::fs::PermissionsExt;

        let (root, id) = adopted_library();
        let before = tree_hash(root.path());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let probe = probe_library(root.path(), Some(&id));

        // Restore before asserting so the tempdir can always clean itself up.
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(probe.state, LibraryState::Offline);
        assert_eq!(probe.reason, LibraryReason::NotWritable);
        assert_eq!(probe.reason.as_str(), "not_writable");
        assert_eq!(tree_hash(root.path()), before);
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_library_is_offline() {
        use std::os::unix::fs::PermissionsExt;

        let (root, id) = adopted_library();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o000)).unwrap();

        let probe = probe_library(root.path(), Some(&id));

        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(probe.state, LibraryState::Offline);
        assert_eq!(probe.reason, LibraryReason::NotReadable);
        assert_eq!(probe.reason.as_str(), "not_readable");
    }

    #[test]
    fn a_file_where_the_library_should_be_is_offline() {
        let parent = tempfile::tempdir().unwrap();
        let not_a_dir = parent.path().join("library");
        fs::write(&not_a_dir, b"not a directory").unwrap();

        let probe = probe_library(&not_a_dir, None);

        assert_eq!(probe.reason, LibraryReason::NotReadable);
    }

    #[test]
    fn adoption_is_idempotent() {
        let (root, id) = adopted_library();
        let again = adopt_library(root.path()).unwrap();
        assert_eq!(again, id, "re-adopting must keep the original identity");
    }

    // ── runtime availability state ──

    #[test]
    fn dto_carries_state_reason_path_and_identity() {
        let dto = LibraryAvailability {
            state: LibraryState::Offline,
            reason: LibraryReason::MissingPath,
            configured_path: PathBuf::from("/Volumes/Ext/Library"),
            library_id: None,
        }
        .to_dto();

        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["state"], "offline");
        assert_eq!(json["reason"], "missing_path");
        assert_eq!(json["configured_path"], "/Volumes/Ext/Library");
        assert!(json["library_id"].is_null());
    }

    #[test]
    fn online_dto_reports_the_stable_ok_reason_and_identity() {
        let dto = LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: PathBuf::from("/Users/me/.skills-manager"),
            library_id: Some("lib-1".to_string()),
        }
        .to_dto();

        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["state"], "online");
        assert_eq!(json["reason"], "ok");
        assert_eq!(json["library_id"], "lib-1");
    }

    #[test]
    fn guard_rejects_while_offline_with_a_dedicated_error_kind() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        // The online branch re-probes the configured base. Without an override
        // that is the developer's real Library, so this test would fail on any
        // machine whose configured Library happens to be an unplugged volume.
        let (root, id) = adopted_library();
        crate::core::central_repo::set_test_base_dir_override(Some(root.path().to_path_buf()));

        set_availability(LibraryAvailability {
            state: LibraryState::Offline,
            reason: LibraryReason::IdentityMismatch,
            configured_path: root.path().to_path_buf(),
            library_id: None,
        });
        let rejected = ensure_library_online().expect_err("offline must fail closed");

        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: root.path().to_path_buf(),
            library_id: Some(id),
        });
        let allowed = ensure_library_online();

        crate::core::central_repo::set_test_base_dir_override(None);
        set_availability(restore);

        assert_eq!(rejected.kind, crate::core::error::ErrorKind::LibraryOffline);
        let json = serde_json::to_value(&rejected).unwrap();
        assert_eq!(
            json["kind"], "library_offline",
            "the frontend branches on this exact value"
        );
        assert!(
            rejected.message.contains("identity_mismatch"),
            "the reason code travels with the error: {}",
            rejected.message
        );
        assert!(allowed.is_ok(), "online must not be rejected");
    }

    // ── polling ──

    #[test]
    fn poll_takes_a_vanished_library_offline() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();
        let root_path = root.path().to_path_buf();
        crate::core::central_repo::set_test_base_dir_override(Some(root_path.clone()));
        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: root_path.clone(),
            library_id: Some(id),
        });

        // Nothing else observes an unmount: a vanished root stops producing
        // watcher events, so the poll is what turns the UI offline.
        drop(root);
        let after = poll_availability();

        crate::core::central_repo::set_test_base_dir_override(None);
        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::MissingPath);
        assert_eq!(
            after.configured_path, root_path,
            "the offline banner still has to name the Library that went away"
        );
    }

    #[test]
    fn poll_never_restores_an_offline_library() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, _) = adopted_library();
        crate::core::central_repo::set_test_base_dir_override(Some(root.path().to_path_buf()));
        // Offline even though the root is present and healthy: only an explicit
        // Retry may clear that verdict, so a background poll must not reconnect.
        set_availability(offline_state(root.path()));

        let after = poll_availability();

        crate::core::central_repo::set_test_base_dir_override(None);
        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::MissingPath);
    }

    #[test]
    fn poll_leaves_a_healthy_library_online_without_writing() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();
        let before = tree_hash(root.path());
        crate::core::central_repo::set_test_base_dir_override(Some(root.path().to_path_buf()));
        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: root.path().to_path_buf(),
            library_id: Some(id),
        });

        let after = poll_availability();

        crate::core::central_repo::set_test_base_dir_override(None);
        set_availability(restore);

        assert!(after.is_online(), "{after:?}");
        assert_eq!(
            tree_hash(root.path()),
            before,
            "polling runs on every page open; it must never write"
        );
    }

    // ── reconnect ──

    /// Records which reconnect steps ran, and can fail either of them.
    struct FakeSteps {
        refresh: std::sync::Mutex<Result<()>>,
        watcher: std::sync::Mutex<Result<()>>,
        ran: std::sync::Mutex<Vec<&'static str>>,
    }

    impl FakeSteps {
        fn succeeding() -> Self {
            Self {
                refresh: std::sync::Mutex::new(Ok(())),
                watcher: std::sync::Mutex::new(Ok(())),
                ran: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn failing_refresh() -> Self {
            let steps = Self::succeeding();
            *steps.refresh.lock().unwrap() = Err(anyhow::anyhow!("metadata refresh failed"));
            steps
        }

        fn failing_watcher() -> Self {
            let steps = Self::succeeding();
            *steps.watcher.lock().unwrap() = Err(anyhow::anyhow!("watcher restart failed"));
            steps
        }

        fn ran(&self) -> Vec<&'static str> {
            self.ran.lock().unwrap().clone()
        }
    }

    impl ReconnectSteps for FakeSteps {
        fn refresh_metadata(&self) -> Result<()> {
            self.ran.lock().unwrap().push("refresh");
            std::mem::replace(&mut *self.refresh.lock().unwrap(), Ok(()))
        }

        fn restart_watcher(&self) -> Result<()> {
            self.ran.lock().unwrap().push("watcher");
            std::mem::replace(&mut *self.watcher.lock().unwrap(), Ok(()))
        }
    }

    fn offline_state(path: &Path) -> LibraryAvailability {
        LibraryAvailability {
            state: LibraryState::Offline,
            reason: LibraryReason::MissingPath,
            configured_path: path.to_path_buf(),
            library_id: None,
        }
    }

    #[test]
    fn retry_goes_online_only_after_every_step_succeeds() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();
        set_availability(offline_state(root.path()));

        let steps = FakeSteps::succeeding();
        let after = retry_library(root.path(), Some(&id), &steps);

        set_availability(restore);

        assert!(after.is_online(), "{after:?}");
        assert_eq!(after.reason, LibraryReason::Ok);
        assert_eq!(after.library_id.as_deref(), Some(id.as_str()));
        assert_eq!(
            steps.ran(),
            vec!["refresh", "watcher"],
            "metadata refresh runs before the watcher restarts"
        );
    }

    #[test]
    fn retry_against_a_different_library_stays_offline_and_runs_nothing() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, _) = adopted_library();
        let before = tree_hash(root.path());
        set_availability(offline_state(root.path()));

        let steps = FakeSteps::succeeding();
        let after = retry_library(root.path(), Some("an-identity-from-another-volume"), &steps);

        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::IdentityMismatch);
        assert!(
            steps.ran().is_empty(),
            "a mismatched Library must not be refreshed or watched"
        );
        assert_eq!(
            tree_hash(root.path()),
            before,
            "a failed retry must not modify the directory"
        );
    }

    #[test]
    fn retry_stays_offline_when_metadata_refresh_fails() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();
        set_availability(offline_state(root.path()));

        let steps = FakeSteps::failing_refresh();
        let after = retry_library(root.path(), Some(&id), &steps);
        let published = availability();

        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::RefreshFailed);
        assert_eq!(
            steps.ran(),
            vec!["refresh"],
            "the watcher must not start after a failed refresh"
        );
        assert_eq!(
            published.state,
            LibraryState::Offline,
            "no partially-online state is published"
        );
    }

    #[test]
    fn retry_stays_offline_when_the_watcher_cannot_restart() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();
        set_availability(offline_state(root.path()));

        let steps = FakeSteps::failing_watcher();
        let after = retry_library(root.path(), Some(&id), &steps);

        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::WatcherFailed);
        assert_eq!(steps.ran(), vec!["refresh", "watcher"]);
    }

    #[test]
    fn retry_on_a_missing_volume_stays_offline_and_creates_nothing() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let parent = tempfile::tempdir().unwrap();
        let absent = parent.path().join("Ext/Library");
        set_availability(offline_state(&absent));

        let steps = FakeSteps::succeeding();
        let after = retry_library(&absent, Some("lib-1"), &steps);

        set_availability(restore);

        assert_eq!(after.reason, LibraryReason::MissingPath);
        assert!(steps.ran().is_empty());
        assert!(!absent.exists(), "retry must not create the path");
    }

    #[test]
    fn going_offline_pauses_watching_and_a_good_retry_resumes_it() {
        // An unmounted volume emits a removal event for every file it held.
        // Watching through that would look exactly like the user deleting the
        // library, so the watcher must stop before those events arrive.
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();

        mark_offline(LibraryReason::MissingPath);
        let paused_while_offline = crate::core::file_watcher::watching_paused();

        let steps = FakeSteps::succeeding();
        retry_library(root.path(), Some(&id), &steps);
        let paused_after_retry = crate::core::file_watcher::watching_paused();

        set_availability(restore);
        crate::core::file_watcher::resume_watching();

        assert!(paused_while_offline, "offline must pause watching");
        assert!(!paused_after_retry, "a verified retry must resume watching");
    }

    #[test]
    fn a_failed_retry_leaves_watching_paused() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();
        let (root, id) = adopted_library();

        mark_offline(LibraryReason::MissingPath);
        let steps = FakeSteps::failing_refresh();
        retry_library(root.path(), Some(&id), &steps);
        let still_paused = crate::core::file_watcher::watching_paused();

        set_availability(restore);
        crate::core::file_watcher::resume_watching();

        assert!(still_paused, "watching stays paused until the retry succeeds");
    }

    #[test]
    fn availability_is_shared_across_threads() {
        let _guard = crate::core::central_repo::test_base_dir_lock();
        let restore = availability();

        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: PathBuf::from("/Volumes/Ext/Library"),
            library_id: Some("lib-1".to_string()),
        });

        let readers: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| availability().state))
            .collect();
        mark_offline(LibraryReason::MissingPath);
        for reader in readers {
            reader.join().unwrap();
        }

        let after = availability();
        set_availability(restore);

        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::MissingPath);
        assert_eq!(
            after.configured_path,
            PathBuf::from("/Volumes/Ext/Library"),
            "a runtime transition keeps the configured path"
        );
    }
}
