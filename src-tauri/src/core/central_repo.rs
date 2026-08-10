use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use walkdir::WalkDir;

const CONFIG_FILE_NAME: &str = "repo-config.json";

static BASE_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static SKILLS_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static STARTUP_WARNINGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static STARTUP_ERROR_LOG: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn push_startup_warning(code: &str) {
    let mut warnings = STARTUP_WARNINGS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !warnings.iter().any(|w| w == code) {
        warnings.push(code.to_string());
    }
}

/// Warning codes recorded while resolving the central repository at startup.
/// The frontend maps them to localized banner text (`settings.repoWarning_*`).
pub fn startup_warnings() -> Vec<String> {
    STARTUP_WARNINGS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Record a detailed startup error for later logging. `ensure_central_repo`
/// runs before `tauri_plugin_log` is installed (see `run()` in lib.rs), so a
/// `log::error!` here is swallowed by the default no-op logger. Stash the
/// detail and let `setup` flush it once the real logger exists.
fn record_startup_error(message: String) {
    STARTUP_ERROR_LOG
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(message);
}

/// Drain the startup errors stashed by [`record_startup_error`]. Called from
/// `tauri::Builder::setup` once the logger is up so the detail lands in the log
/// file that a support bundle collects.
pub fn take_startup_errors() -> Vec<String> {
    let mut guard = STARTUP_ERROR_LOG
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut guard)
}

/// Global mutex shared by every test that mutates the base-dir override via
/// [`set_test_base_dir_override`]. The override is process-wide static state,
/// so any two tests holding their own per-module locks can still race. Tests
/// must take this guard before calling `set_test_base_dir_override` and keep
/// it alive until they restore the previous value.
#[cfg(test)]
static TEST_BASE_DIR_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn test_base_dir_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_BASE_DIR_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Current on-disk config version. v1 (an absent or `0` field) stores a single
/// `repo_path` that meant "everything lives here". v2 splits that into internal
/// application state plus a `library_base` that may sit on an external volume.
const CONFIG_VERSION: u32 = 2;

/// Scratch directory used while adopting a legacy layout. State is copied here
/// first and only moved into place once verified, so an interrupted run never
/// leaves a half-copy where the real database belongs.
const MIGRATION_STAGING_DIR: &str = ".migration-staging";

/// Application state worth carrying over from a legacy base. Cache and logs are
/// rebuilt on demand, so copying them would only slow the upgrade down.
const MIGRATED_STATE_ENTRIES: [&str; 2] = ["skills-manager.db", "scenarios"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoPathConfig {
    #[serde(default)]
    version: u32,
    /// v1 field. Retained after the upgrade so a rollback can still find the
    /// location the state was adopted from.
    repo_path: Option<String>,
    pending_migration_from: Option<String>,
    /// v2: the Library content root. `None` means the internal default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library_base: Option<String>,
    /// v2: identity expected at the Library root. A root that does not carry
    /// this identity is a different volume, however matching its path looks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library_id: Option<String>,
    /// v2: how the v1 → v2 upgrade ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    migration: Option<MigrationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MigrationRecord {
    /// The v1 `repo_path` the state was adopted from. Never deleted by this
    /// upgrade, so it stays usable as a rollback source.
    legacy_source: String,
    status: MigrationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MigrationStatus {
    Completed,
    PendingOffline,
    Blocked,
}

/// Result of adopting a legacy v1 layout. Every non-`Completed` variant leaves
/// both locations exactly as they were.
#[derive(Debug)]
enum LegacyMigration {
    /// Already on v2, or there is nothing external to adopt.
    NotNeeded,
    /// State copied, verified, and recorded in the config.
    Completed,
    /// The legacy volume is absent; retry on a later launch.
    PendingOffline,
    /// Both sides hold state that cannot be reconciled automatically.
    Blocked(String),
    /// The copy could not be verified (unreadable or corrupt source state).
    Failed(String),
}

fn default_base_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".skills-manager")
}

fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(default_base_dir)
        .join("skills-manager")
        .join(CONFIG_FILE_NAME)
}

/// Distinguishes "no config file" (normal fresh install) from "config file
/// exists but cannot be used" (must never be silently treated as a fresh
/// install — that is how a configured library turns into an empty default
/// one and users report "all my skills are gone", issue #228 review).
#[derive(Debug)]
enum ConfigState {
    Missing,
    Valid(RepoPathConfig),
    Invalid(String),
}

fn load_config_state_from(path: &Path) -> ConfigState {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ConfigState::Missing,
        Err(err) => {
            return ConfigState::Invalid(format!("cannot read {}: {err}", path.display()));
        }
    };
    match serde_json::from_str(&raw) {
        Ok(config) => ConfigState::Valid(config),
        Err(err) => ConfigState::Invalid(format!("corrupt JSON in {}: {err}", path.display())),
    }
}

fn load_config_state() -> ConfigState {
    load_config_state_from(&config_file_path())
}

fn load_config() -> RepoPathConfig {
    match load_config_state() {
        ConfigState::Valid(config) => config,
        ConfigState::Missing | ConfigState::Invalid(_) => RepoPathConfig::default(),
    }
}

fn save_config(config: &RepoPathConfig) -> Result<()> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

fn normalize_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Path cannot be empty"));
    }

    let expanded = if trimmed == "~" {
        dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?
    } else if trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        dirs::home_dir()
            .ok_or_else(|| anyhow!("Cannot determine home directory"))?
            .join(&trimmed[2..])
    } else {
        PathBuf::from(trimmed)
    };

    if !expanded.is_absolute() {
        return Err(anyhow!("Central repository path must be absolute"));
    }

    let mut normalized = PathBuf::new();
    for component in expanded.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

/// Identity the configured Library is expected to carry. `None` before the
/// Library has been adopted, which is what lets a legacy root be adopted once.
pub fn configured_library_id() -> Option<String> {
    load_config().library_id
}

/// Persist the identity of the Library that was just adopted.
pub fn record_library_identity(id: &str) -> Result<()> {
    let mut config = load_config();
    config.library_id = Some(id.to_string());
    save_config(&config)
}

/// The Library root the user configured, if any. Reads the v2 field once the
/// config has been upgraded and falls back to the v1 `repo_path` before that.
pub fn configured_base_dir() -> Option<PathBuf> {
    let config = load_config();
    let configured = if config.version >= CONFIG_VERSION {
        config.library_base
    } else {
        config.repo_path
    };
    configured.and_then(|path| normalize_path(&path).ok())
}

/// Where application state and Library content are resolved to.
///
/// The two halves are deliberately separate: SQLite, scenarios, cache, and logs
/// stay on internal storage so AgentDeck still starts when a configured external
/// Library volume is absent, while only the Library content root follows the
/// configured path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoLayout {
    state_base: PathBuf,
    library_base: PathBuf,
}

impl RepoLayout {
    fn skills_dir(&self) -> PathBuf {
        self.library_base.join("skills")
    }

    fn scenarios_dir(&self) -> PathBuf {
        self.state_base.join("scenarios")
    }

    fn cache_dir(&self) -> PathBuf {
        self.state_base.join("cache")
    }

    fn logs_dir(&self) -> PathBuf {
        self.state_base.join("logs")
    }

    fn db_path(&self) -> PathBuf {
        self.state_base.join("skills-manager.db")
    }

    fn library_is_external(&self) -> bool {
        self.library_base != self.state_base
    }
}

fn resolve_layout(
    config: &RepoPathConfig,
    internal_base: &Path,
    cli_override: Option<&Path>,
) -> RepoLayout {
    // An explicit CLI root owns both halves. `--skills-root` already namespaces
    // its state under the default base via `external_base_dir`, and `--path`
    // callers chose that location for everything; the app's config-driven split
    // must not second-guess either.
    if let Some(root) = cli_override {
        return RepoLayout {
            state_base: root.to_path_buf(),
            library_base: root.to_path_buf(),
        };
    }

    // v2 states the Library root explicitly; v1 only knew `repo_path`, which
    // named a base that held both halves.
    let configured = if config.version >= CONFIG_VERSION {
        config.library_base.as_deref()
    } else {
        config.repo_path.as_deref()
    };
    let library_base = configured
        .and_then(|raw| normalize_path(raw).ok())
        .unwrap_or_else(|| internal_base.to_path_buf());

    RepoLayout {
        state_base: internal_base.to_path_buf(),
        library_base,
    }
}

/// Directories startup may create. An external Library root is excluded on
/// purpose: creating it would turn an unmounted volume into an empty directory
/// that reindex and sync later read as "the library was deleted".
fn startup_dirs_to_create(layout: &RepoLayout) -> Vec<PathBuf> {
    let mut dirs = vec![
        layout.scenarios_dir(),
        layout.cache_dir(),
        layout.logs_dir(),
    ];
    if !layout.library_is_external() {
        dirs.push(layout.skills_dir());
    }
    dirs
}

fn cli_base_override() -> Option<PathBuf> {
    BASE_DIR_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn current_layout() -> RepoLayout {
    resolve_layout(
        &load_config(),
        &default_base_dir(),
        cli_base_override().as_deref(),
    )
}

/// Root of the application's own state (SQLite, scenarios, cache, logs).
/// Always internal unless a CLI override names another root.
pub fn base_dir() -> PathBuf {
    current_layout().state_base
}

/// Root that contains the Library's `skills/` directory. This is what the user
/// configures and what Settings shows; it may live on an external volume.
pub fn library_base_dir() -> PathBuf {
    current_layout().library_base
}

/// Whether an explicit runtime base-dir override is active (CLI `--skills-root`
/// / `--path`). Startup migration is skipped when it is — the caller chose a
/// specific library and the app's shared pending-migration marker doesn't apply.
pub(crate) fn base_dir_override_active() -> bool {
    BASE_DIR_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

pub fn set_runtime_base_dir_override(path: Option<PathBuf>) {
    *BASE_DIR_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = path;
}

pub fn set_runtime_skills_dir_override(path: Option<PathBuf>) {
    *SKILLS_DIR_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = path;
}

#[cfg(test)]
pub(crate) fn set_test_base_dir_override(path: Option<PathBuf>) {
    use crate::core::library_availability::{
        LibraryAvailability, LibraryReason, LibraryState, set_availability,
    };

    set_runtime_base_dir_override(path.clone());
    set_runtime_skills_dir_override(None);
    // A test repo stands in for an online Library, so it has to exist on disk:
    // mutating flows re-probe the root before writing, and an absent directory
    // is exactly what "offline" means. Offline cases still opt in explicitly.
    if let Some(base) = path.as_deref() {
        let _ = fs::create_dir_all(base);
    }
    set_availability(LibraryAvailability {
        state: LibraryState::Online,
        reason: LibraryReason::Ok,
        configured_path: path.unwrap_or_else(default_base_dir),
        library_id: Some("test-library".to_string()),
    });
}

pub fn skills_dir() -> PathBuf {
    if let Some(path) = SKILLS_DIR_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone()
    {
        return path;
    }
    current_layout().skills_dir()
}

/// Derive a stable per-skills-root state directory under the user's default base.
///
/// CLI's `--skills-root` lets agents operate on an external skills checkout
/// (e.g. a freshly cloned `my-skills`) without touching the app's default repo.
/// The manager still needs a home for its DB, scenarios, cache, and logs — but
/// putting that state inside the external checkout would pollute the user's
/// repo, and putting it in the parent directory would silently litter wherever
/// the user happened to clone. Instead, namespace the state under
/// `<default-base>/external/<sanitized-name>-<short-hash>/`, keyed by the
/// canonical path of the skills root so repeat invocations reuse the same DB.
pub fn external_base_dir(skills_root: &Path) -> PathBuf {
    // canonicalize() requires the path to exist. For not-yet-cloned targets we
    // still want a stable namespace, so fall back to absolutizing + lexically
    // normalizing the path. Without this, `./my-skills`, `my-skills`, and
    // `a/../my-skills` would hash to different namespaces despite resolving
    // to the same location.
    let canonical = match skills_root.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let absolute = if skills_root.is_absolute() {
                skills_root.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(skills_root))
                    .unwrap_or_else(|_| skills_root.to_path_buf())
            };
            lexically_normalize(&absolute)
        }
    };
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("external");
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let short_hash: String = digest.iter().take(5).map(|b| format!("{:02x}", b)).collect();
    default_base_dir()
        .join("external")
        .join(format!("{}-{}", sanitize_dir_name(name), short_hash))
}

/// Lexically normalize `.` and `..` segments without touching the filesystem.
/// `..` over a normal segment cancels it; `..` over a root or another `..`
/// is preserved (so we don't pretend to escape the filesystem root).
fn lexically_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {
                    // can't go above root — drop the `..`
                }
                _ => out.push(comp),
            },
            other => out.push(other),
        }
    }
    out.iter().collect()
}

fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "external".to_string()
    } else {
        cleaned
    }
}

pub fn scenarios_dir() -> PathBuf {
    current_layout().scenarios_dir()
}

pub fn cache_dir() -> PathBuf {
    current_layout().cache_dir()
}

pub fn logs_dir() -> PathBuf {
    current_layout().logs_dir()
}

pub fn db_path() -> PathBuf {
    current_layout().db_path()
}

pub fn set_base_dir_override(path: Option<String>) -> Result<PathBuf> {
    // Changing this path moves the Library, not the application state, so the
    // comparison baseline is the Library root.
    let current = library_base_dir();
    let mut config = load_config();

    // The actual on-disk data location can differ from `current` when the user
    // already changed the path once but hasn't restarted yet — `current` then
    // reflects the unsatisfied future target stored in `repo_path`, while the
    // data still sits at `pending_migration_from`. Track the true location so
    // multiple changes before restart still migrate from the right source.
    let data_location = match &config.pending_migration_from {
        Some(src) => match normalize_path(src) {
            Ok(path) if path.is_dir() => path,
            _ => current.clone(),
        },
        None => current.clone(),
    };

    let (next, persist_repo_path) = match path {
        Some(raw) => (normalize_path(&raw)?, true),
        None => (default_base_dir(), false),
    };

    apply_base_dir_change(&mut config, &next, &data_location, persist_repo_path);
    save_config(&config)?;
    Ok(next)
}

/// Point the config at `next`, recording what has to be migrated from
/// `data_location`. Split out from [`set_base_dir_override`] so the field
/// bookkeeping is testable without writing to the real user config.
fn apply_base_dir_change(
    config: &mut RepoPathConfig,
    next: &Path,
    data_location: &Path,
    persist_repo_path: bool,
) {
    let persisted = if persist_repo_path {
        Some(next.to_string_lossy().to_string())
    } else {
        None
    };
    if config.version >= CONFIG_VERSION {
        config.library_base = persisted;
    } else {
        config.repo_path = persisted;
    }
    let root_changed = next != data_location;
    config.pending_migration_from = if root_changed {
        Some(data_location.to_string_lossy().to_string())
    } else {
        None
    };
    if root_changed {
        // The stored identity belongs to the root being left behind. Keeping it
        // makes the freshly chosen root — which carries no marker until the
        // migration copies one — probe as `identity_mismatch`, so the app comes
        // back offline after the restart it just asked the user for.
        config.library_id = None;
    }
}

/// Whether `path` holds anything the migration must not overwrite.
///
/// The Library marker does not count. Startup probes availability before it
/// migrates, and probing an unclaimed root adopts it — so by the time the
/// migration looks, a freshly chosen empty root already carries a marker this
/// app just wrote. Treating that as "the user has data here" blocks every
/// migration into a new Library.
fn directory_has_entries(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(path)? {
        if entry?.file_name() == crate::core::library_availability::MARKER_FILE_NAME {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Whether two paths resolve to the same directory. Falls back to a lexical
/// comparison when either side can't be canonicalized (e.g. the target does not
/// exist yet), so a purely cosmetic difference (case, `8.3` names, a symlink)
/// isn't mistaken for a real relocation.
fn paths_are_same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// What the caller should do after attempting a pending central-repo move.
#[derive(Debug)]
enum MigrationOutcome {
    /// No move was pending, or it completed. Run against the configured base.
    Proceed,
    /// The move could not complete safely; the intact library still lives at
    /// this path. Run this session against it and retry on the next launch.
    UseSource(PathBuf),
}

/// Try to satisfy a pending central-repository relocation.
///
/// This runs before the logger, the panic hook, and the window exist (see
/// `run()` in lib.rs), so it must never return an error that would panic the
/// process into a windowless death (#252). Every failure instead records a
/// startup warning + a deferred log line and falls back to the source, where
/// the user's data is known to be intact. It mutates `config` in place but
/// does NOT persist it — the caller saves once, which also keeps this unit
/// testable without touching the real config file.
fn migrate_repo_if_needed(config: &mut RepoPathConfig, current_base: &Path) -> MigrationOutcome {
    let Some(source_raw) = config.pending_migration_from.clone() else {
        return MigrationOutcome::Proceed;
    };
    let source = match normalize_path(&source_raw) {
        Ok(path) => path,
        Err(err) => {
            // The stored path is unusable, so the move can never proceed. Drop
            // the marker to stop retrying every launch and run against target.
            record_startup_error(format!(
                "central repo: pending migration source {source_raw:?} is invalid ({err}); dropping it"
            ));
            config.pending_migration_from = None;
            return MigrationOutcome::Proceed;
        }
    };

    // Nothing left to move: the source is gone (moved already, or the old
    // location was removed), or source and target are the same directory.
    // Compare canonically, not just lexically — on a case-insensitive volume
    // `D:\Skills` and `d:\skills` are one directory (likewise 8.3 vs long, or a
    // symlink), and a lexical mismatch would otherwise loop forever on
    // `migration_incomplete`, telling the user to empty their own library.
    if !source.exists() || paths_are_same_dir(&source, current_base) {
        config.pending_migration_from = None;
        return MigrationOutcome::Proceed;
    }

    // A target nested inside the source can never be a valid destination.
    if current_base.starts_with(&source) {
        record_startup_error(format!(
            "central repo: migration target {} is inside source {}; keeping data at the source",
            current_base.display(),
            source.display()
        ));
        push_startup_warning("migration_incomplete");
        return MigrationOutcome::UseSource(source);
    }

    // Only ever move into an absent/empty target — never blind-merge. A
    // non-empty target is either a real library we must not overwrite or debris
    // from a failed attempt we cannot tell apart; keeping the user on their
    // intact source is lossless, overwriting is not. A fresh target also means
    // the recursive copy only ever creates new files, so it can never hit the
    // read-only git pack files that overwriting bricked startup on (#252).
    let target_empty = match directory_has_entries(current_base) {
        Ok(has_entries) => !has_entries,
        Err(err) => {
            record_startup_error(format!(
                "central repo: cannot inspect migration target {} ({err}); keeping data at source {}",
                current_base.display(),
                source.display()
            ));
            push_startup_warning("migration_incomplete");
            return MigrationOutcome::UseSource(source);
        }
    };
    if !target_empty {
        record_startup_error(format!(
            "central repo: migration target {} is not empty; keeping data at source {}",
            current_base.display(),
            source.display()
        ));
        push_startup_warning("migration_incomplete");
        return MigrationOutcome::UseSource(source);
    }

    // Require the target to already exist. A path that is not there is either
    // an unmounted external volume or a folder the user never created; creating
    // it would put an empty library on a mountpoint, which downstream reindex
    // and sync read as "the library was emptied".
    if !current_base.exists() {
        record_startup_error(format!(
            "central repo: migration target {} does not exist (volume not mounted?); keeping data at source {}",
            current_base.display(),
            source.display()
        ));
        push_startup_warning("migration_incomplete");
        return MigrationOutcome::UseSource(source);
    }

    // Same volume: an atomic rename moves the whole tree cheaply. Cross volume
    // (or a rename the OS refuses): copy into the empty target. Because the
    // target is empty, no existing file is ever overwritten.
    if fs::rename(&source, current_base).is_err() {
        if let Err(err) = copy_dir_recursive(&source, current_base) {
            record_startup_error(format!(
                "central repo: migration copy from {} to {} failed ({err:#}); keeping data at source",
                source.display(),
                current_base.display()
            ));
            push_startup_warning("migration_incomplete");
            return MigrationOutcome::UseSource(source);
        }
    }

    config.pending_migration_from = None;
    // The move carries the source's marker across, so the identity the target
    // now advertises is the one the config has to expect. Leaving the config on
    // the id adopted before the move makes the freshly migrated Library probe
    // as somebody else's volume.
    config.library_id = crate::core::library_availability::read_library_id(current_base);
    MigrationOutcome::Proceed
}

fn file_hash(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().to_vec())
}

/// A staged copy is only trusted once the database bytes match the source and
/// the staged file actually opens as the application's database. Verifying the
/// staged copy rather than the source keeps the legacy location read-only.
fn verify_staged_state(source: &Path, staging: &Path) -> Result<()> {
    let source_db = source.join(MIGRATED_STATE_ENTRIES[0]);
    if !source_db.exists() {
        return Ok(());
    }
    let staged_db = staging.join(MIGRATED_STATE_ENTRIES[0]);
    if file_hash(&source_db)? != file_hash(&staged_db)? {
        return Err(anyhow!("copied database does not match the source"));
    }

    let store = crate::core::skill_store::SkillStore::new(&staged_db)
        .context("copied database does not open")?;
    let skills = store.get_all_skills().context("copied database has no readable skills")?;
    let scenarios = store
        .get_all_scenarios()
        .context("copied database has no readable scenarios")?;
    log::info!(
        "central repo: verified migrated state ({} skills, {} scenarios)",
        skills.len(),
        scenarios.len()
    );
    Ok(())
}

/// Adopt a legacy v1 `repo_path` layout: copy the application state it holds
/// onto internal storage, verify the copy, and only then record the legacy base
/// as the Library root. The source is never modified or deleted, so a failure
/// at any point leaves the user on their intact v1 layout.
fn migrate_legacy_layout(config: &mut RepoPathConfig, internal_base: &Path) -> LegacyMigration {
    if config.version >= CONFIG_VERSION {
        return LegacyMigration::NotNeeded;
    }

    let Some(raw) = config.repo_path.clone() else {
        // Default internal layout: nothing to move, just record the new shape.
        config.version = CONFIG_VERSION;
        return LegacyMigration::NotNeeded;
    };
    let Ok(legacy) = normalize_path(&raw) else {
        // `ensure_central_repo` already warns about an unusable configured path;
        // there is nothing here to adopt.
        config.version = CONFIG_VERSION;
        return LegacyMigration::NotNeeded;
    };
    if paths_are_same_dir(&legacy, internal_base) {
        config.version = CONFIG_VERSION;
        return LegacyMigration::NotNeeded;
    }
    // The version field stays on v1 for both deferrals below, so the next
    // launch re-evaluates; the record only explains why this one stopped.
    let mark = |config: &mut RepoPathConfig, status: MigrationStatus| {
        config.migration = Some(MigrationRecord {
            legacy_source: legacy.to_string_lossy().to_string(),
            status,
        });
    };

    if !legacy.exists() {
        mark(config, MigrationStatus::PendingOffline);
        return LegacyMigration::PendingOffline;
    }

    // Never reconcile two populated locations automatically — one of them would
    // have to lose, and neither is safe to pick without the user.
    for entry in MIGRATED_STATE_ENTRIES {
        if internal_base.join(entry).exists() && legacy.join(entry).exists() {
            mark(config, MigrationStatus::Blocked);
            return LegacyMigration::Blocked(entry.to_string());
        }
    }

    let staging = internal_base.join(MIGRATION_STAGING_DIR);
    let _ = fs::remove_dir_all(&staging);
    if let Err(err) = fs::create_dir_all(&staging) {
        return LegacyMigration::Failed(format!("cannot create migration staging: {err}"));
    }

    for entry in MIGRATED_STATE_ENTRIES {
        let source = legacy.join(entry);
        if !source.exists() {
            continue;
        }
        let target = staging.join(entry);
        let copied = if source.is_dir() {
            copy_dir_recursive(&source, &target)
        } else {
            fs::copy(&source, &target)
                .map(|_| ())
                .with_context(|| format!("cannot copy {}", source.display()))
        };
        if let Err(err) = copied {
            let _ = fs::remove_dir_all(&staging);
            return LegacyMigration::Failed(format!("{err:#}"));
        }
    }

    if let Err(err) = verify_staged_state(&legacy, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return LegacyMigration::Failed(format!("{err:#}"));
    }

    for entry in MIGRATED_STATE_ENTRIES {
        let staged = staging.join(entry);
        if !staged.exists() {
            continue;
        }
        let target = internal_base.join(entry);
        if fs::rename(&staged, &target).is_err() {
            let moved = if staged.is_dir() {
                copy_dir_recursive(&staged, &target)
            } else {
                fs::copy(&staged, &target).map(|_| ()).map_err(Into::into)
            };
            if let Err(err) = moved {
                let _ = fs::remove_dir_all(&staging);
                return LegacyMigration::Failed(format!("{err:#}"));
            }
        }
    }
    let _ = fs::remove_dir_all(&staging);

    // Stamp the adopted root so a later launch can tell this Library apart from
    // whatever else may appear at the same path.
    let library_id = match crate::core::library_availability::adopt_library(&legacy) {
        Ok(id) => id,
        Err(err) => return LegacyMigration::Failed(format!("{err:#}")),
    };

    let legacy_source = legacy.to_string_lossy().to_string();
    config.version = CONFIG_VERSION;
    config.library_base = Some(legacy_source.clone());
    config.library_id = Some(library_id);
    config.migration = Some(MigrationRecord {
        legacy_source,
        status: MigrationStatus::Completed,
    });
    LegacyMigration::Completed
}

/// Run the v1 → v2 config upgrade and persist whatever it decided. Runs before
/// the logger exists, so failures record a startup warning instead of erroring.
fn upgrade_legacy_config(config: &mut RepoPathConfig) {
    let internal_base = default_base_dir();
    if let Err(err) = fs::create_dir_all(&internal_base) {
        record_startup_error(format!(
            "central repo: cannot prepare internal state at {} ({err})",
            internal_base.display()
        ));
        return;
    }

    match migrate_legacy_layout(config, &internal_base) {
        LegacyMigration::NotNeeded | LegacyMigration::Completed => {}
        LegacyMigration::PendingOffline => {
            record_startup_error(
                "central repo: configured library is offline; deferring the layout upgrade"
                    .to_string(),
            );
        }
        LegacyMigration::Blocked(detail) => {
            record_startup_error(format!(
                "central repo: layout upgrade blocked, both locations hold {detail}"
            ));
            push_startup_warning("migration_blocked");
        }
        LegacyMigration::Failed(detail) => {
            record_startup_error(format!("central repo: layout upgrade failed ({detail})"));
            push_startup_warning("migration_incomplete");
        }
    }

    if let Err(err) = save_config(config) {
        record_startup_error(format!(
            "central repo: failed to persist the upgraded config ({err}); it may retry next launch"
        ));
    }
}

pub fn ensure_central_repo() -> Result<()> {
    // A config file that exists but cannot be used means the app is about to
    // run against the default location even though the user configured (and
    // populated) another one. Never let that pass silently — it presents as
    // "the library was rebuilt empty, all skills lost" (#228 review).
    let mut config = match load_config_state() {
        ConfigState::Valid(config) => {
            if let Some(raw) = config.repo_path.as_deref() {
                if let Err(err) = normalize_path(raw) {
                    log::error!(
                        "central repo: configured repo_path {raw:?} is invalid ({err}); \
                         falling back to the default location"
                    );
                    push_startup_warning("repo_path_invalid");
                }
            }
            config
        }
        ConfigState::Missing => RepoPathConfig::default(),
        ConfigState::Invalid(detail) => {
            log::error!(
                "central repo: config is unreadable ({detail}); \
                 falling back to the default location"
            );
            push_startup_warning("config_unreadable");
            RepoPathConfig::default()
        }
    };

    // Only auto-migrate the app's own config-driven base. When a runtime base
    // override is active (CLI `--skills-root` / `--path`), the pending marker in
    // the shared config belongs to a different library and must not be applied
    // to — or override — the explicitly chosen root. The app's own startup never
    // sets an override before this point, so the #252 path is unaffected.
    if !base_dir_override_active() {
        let pending_before = config.pending_migration_from.clone();
        // The pending marker tracks a Library relocation the user requested, so
        // it targets the Library base — application state no longer moves with
        // it.
        let current_base = library_base_dir();
        let outcome = migrate_repo_if_needed(&mut config, &current_base);
        if config.pending_migration_from != pending_before {
            if let Err(err) = save_config(&config) {
                record_startup_error(format!(
                    "central repo: failed to persist migration state ({err}); it may retry next launch"
                ));
            }
        }
        if let MigrationOutcome::UseSource(source) = outcome {
            // Run this whole session against the intact source library.
            // `base_dir()` (and every dir derived from it) now resolves there,
            // so the code below and the rest of startup stay consistent.
            set_runtime_base_dir_override(Some(source));
        } else {
            // Only adopt the v1 layout once no relocation is still in flight —
            // otherwise the upgrade would read state from a half-moved base.
            upgrade_legacy_config(&mut config);
        }
    }
    // Re-resolve: a fallback override above may have changed the base.
    let layout = current_layout();
    let current_base = layout.library_base.clone();

    // Legacy `.agent-skills` migration must run before create_dir_all below:
    // it renames entries into `current_base` and skips ones that already
    // exist, so pre-created empty dirs would silently swallow it (the old
    // ordering made this branch dead code).
    //
    // An external Library base that is not present means the volume is absent,
    // not that the user needs a fresh library there — creating it would be the
    // empty-mountpoint failure this split exists to prevent.
    let legacy_migration_allowed = !layout.library_is_external() || current_base.exists();
    let legacy_path = dirs::home_dir().map(|home| home.join(".agent-skills"));
    if let Some(old_path) = legacy_path {
        if legacy_migration_allowed && old_path.exists() && !current_base.join("skills").exists() {
            log::info!("Migrating from old path {:?}", old_path);
            fs::create_dir_all(&current_base)?;
            if let Ok(entries) = fs::read_dir(&old_path) {
                for entry in entries.flatten() {
                    let dest = current_base.join(entry.file_name());
                    if !dest.exists() {
                        let _ = fs::rename(entry.path(), &dest);
                    }
                }
            }
        }
    }

    for d in startup_dirs_to_create(&layout) {
        fs::create_dir_all(&d)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── apply_base_dir_change ──

    #[test]
    fn moving_the_library_drops_the_identity_of_the_old_root() {
        let mut config = RepoPathConfig {
            version: CONFIG_VERSION,
            library_base: Some("/Users/me/.skills-manager".to_string()),
            library_id: Some("id-of-the-old-root".to_string()),
            ..RepoPathConfig::default()
        };

        apply_base_dir_change(
            &mut config,
            Path::new("/Volumes/Ext/Library"),
            Path::new("/Users/me/.skills-manager"),
            true,
        );

        assert_eq!(
            config.library_id, None,
            "the new root carries no marker until the migration copies one; \
             keeping the old identity makes it probe as a different Library"
        );
        assert_eq!(
            config.pending_migration_from.as_deref(),
            Some("/Users/me/.skills-manager")
        );
    }

    #[test]
    fn re_selecting_the_same_root_keeps_its_identity() {
        let mut config = RepoPathConfig {
            version: CONFIG_VERSION,
            library_base: Some("/Volumes/Ext/Library".to_string()),
            library_id: Some("id-of-this-root".to_string()),
            ..RepoPathConfig::default()
        };

        apply_base_dir_change(
            &mut config,
            Path::new("/Volumes/Ext/Library"),
            Path::new("/Volumes/Ext/Library"),
            true,
        );

        assert_eq!(
            config.library_id.as_deref(),
            Some("id-of-this-root"),
            "nothing moved, so the adopted identity still applies"
        );
        assert_eq!(config.pending_migration_from, None);
    }

    // ── directory_has_entries ──

    #[test]
    fn a_root_holding_only_the_library_marker_still_counts_as_empty() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path()
                .join(crate::core::library_availability::MARKER_FILE_NAME),
            "{}",
        )
        .unwrap();

        assert!(
            !directory_has_entries(root.path()).unwrap(),
            "startup adopts an unclaimed root before it migrates, so the marker \
             is this app's own bookkeeping — counting it blocks the migration \
             the user just asked for"
        );

        fs::write(root.path().join("SKILL.md"), "# real user data").unwrap();
        assert!(
            directory_has_entries(root.path()).unwrap(),
            "anything else present must still stop the migration"
        );
    }

    // ── migrate_repo_if_needed (#252) ──

    fn config_migrating(source: &Path, target: &Path) -> RepoPathConfig {
        RepoPathConfig {
            repo_path: Some(target.to_string_lossy().to_string()),
            pending_migration_from: Some(source.to_string_lossy().to_string()),
            ..RepoPathConfig::default()
        }
    }

    #[test]
    fn migration_into_empty_target_moves_and_clears_marker() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap(); // exists but empty
        fs::create_dir_all(src.path().join("skills")).unwrap();
        fs::write(src.path().join("skills/s.md"), b"skill").unwrap();

        let mut config = config_migrating(src.path(), dst.path());
        let outcome = migrate_repo_if_needed(&mut config, dst.path());

        assert!(matches!(outcome, MigrationOutcome::Proceed));
        assert_eq!(config.pending_migration_from, None);
        assert_eq!(fs::read(dst.path().join("skills/s.md")).unwrap(), b"skill");
    }

    #[test]
    fn migration_adopts_the_identity_that_moved_with_the_library() {
        // Startup adopts the empty target before migrating, then the move
        // brings the source's marker across. The config has to end up on the
        // identity the target actually advertises, or the Library the user just
        // moved reads as a different volume and comes back offline.
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let marker = crate::core::library_availability::MARKER_FILE_NAME;
        fs::write(
            src.path().join(marker),
            br#"{"id":"identity-that-travels","created_at":1}"#,
        )
        .unwrap();

        let mut config = config_migrating(src.path(), dst.path());
        config.library_id = Some("identity-adopted-before-the-move".to_string());
        let outcome = migrate_repo_if_needed(&mut config, dst.path());

        assert!(matches!(outcome, MigrationOutcome::Proceed));
        assert_eq!(config.library_id.as_deref(), Some("identity-that-travels"));
    }

    #[test]
    fn migration_into_nonempty_target_keeps_source_and_marker() {
        // The whole point of #252's safety: never blind-merge over a
        // non-empty target (real data or failed-attempt debris we can't tell
        // apart). Fall back to the intact source and keep retrying.
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        fs::write(src.path().join("a.txt"), b"src").unwrap();
        fs::write(dst.path().join("existing.txt"), b"dst-data").unwrap();

        let mut config = config_migrating(src.path(), dst.path());
        let outcome = migrate_repo_if_needed(&mut config, dst.path());

        match outcome {
            MigrationOutcome::UseSource(p) => {
                assert_eq!(p, normalize_path(&src.path().to_string_lossy()).unwrap());
            }
            _ => panic!("expected UseSource for a non-empty target"),
        }
        assert!(config.pending_migration_from.is_some(), "marker kept for retry");
        assert_eq!(fs::read(dst.path().join("existing.txt")).unwrap(), b"dst-data");
        assert_eq!(fs::read(src.path().join("a.txt")).unwrap(), b"src");
    }

    #[test]
    #[cfg(unix)]
    fn migration_same_dir_via_symlink_clears_marker() {
        // A cosmetic path difference that resolves to the same directory (here
        // a symlink; on Windows, case / 8.3 names) must not be mistaken for a
        // real relocation — otherwise it loops forever on `migration_incomplete`
        // telling the user to empty their own library.
        let real = tempfile::tempdir().unwrap();
        fs::create_dir_all(real.path().join("skills")).unwrap();
        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("aliased");
        std::os::unix::fs::symlink(real.path(), &link).unwrap();

        let mut config = config_migrating(real.path(), &link);
        let outcome = migrate_repo_if_needed(&mut config, &link);

        assert!(matches!(outcome, MigrationOutcome::Proceed));
        assert_eq!(config.pending_migration_from, None, "same-dir move clears marker");
        // The real library is untouched.
        assert!(real.path().join("skills").exists());
    }

    #[test]
    fn migration_into_an_absent_target_keeps_source_and_creates_nothing() {
        // An external target that does not exist means the volume is absent.
        // Creating it would leave an empty directory on the mountpoint that
        // later flows read as "the library was emptied".
        let src = tempfile::tempdir().unwrap();
        fs::create_dir_all(src.path().join("skills")).unwrap();
        fs::write(src.path().join("skills/s.md"), b"skill").unwrap();
        let parent = tempfile::tempdir().unwrap();
        let absent_target = parent.path().join("Ext/Library");

        let mut config = config_migrating(src.path(), &absent_target);
        let outcome = migrate_repo_if_needed(&mut config, &absent_target);

        match outcome {
            MigrationOutcome::UseSource(p) => {
                assert_eq!(p, normalize_path(&src.path().to_string_lossy()).unwrap());
            }
            other => panic!("expected UseSource for an absent target, got {other:?}"),
        }
        assert!(!absent_target.exists(), "the target must not be created");
        assert!(config.pending_migration_from.is_some(), "marker kept for retry");
        assert_eq!(fs::read(src.path().join("skills/s.md")).unwrap(), b"skill");
    }

    #[test]
    fn migration_with_missing_source_clears_marker() {
        let dst = tempfile::tempdir().unwrap();
        let missing = dst.path().join("does-not-exist");
        let mut config = config_migrating(&missing, dst.path());

        let outcome = migrate_repo_if_needed(&mut config, dst.path());
        assert!(matches!(outcome, MigrationOutcome::Proceed));
        assert_eq!(config.pending_migration_from, None);
    }

    #[test]
    fn no_pending_migration_is_a_noop() {
        let dst = tempfile::tempdir().unwrap();
        let mut config = RepoPathConfig {
            repo_path: Some(dst.path().to_string_lossy().to_string()),
            pending_migration_from: None,
            ..RepoPathConfig::default()
        };
        let outcome = migrate_repo_if_needed(&mut config, dst.path());
        assert!(matches!(outcome, MigrationOutcome::Proceed));
        assert_eq!(config.pending_migration_from, None);
    }

    #[test]
    fn copy_dir_recursive_copies_read_only_source_files() {
        // git pack files (.idx/.pack/.rev) are read-only. Copying them into a
        // fresh target must succeed — the #252 brick only happened when
        // OVERWRITING an existing read-only file, which migration now avoids by
        // only ever moving into an empty target.
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let pack = src.path().join("pack.idx");
        fs::write(&pack, b"packdata").unwrap();
        let mut perms = fs::metadata(&pack).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&pack, perms).unwrap();

        let target = dst.path().join("out");
        copy_dir_recursive(src.path(), &target).unwrap();
        assert_eq!(fs::read(target.join("pack.idx")).unwrap(), b"packdata");
    }

    // ── load_config_state_from ──

    #[test]
    fn config_state_missing_file_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let state = load_config_state_from(&tmp.path().join("repo-config.json"));
        assert!(matches!(state, ConfigState::Missing));
    }

    #[test]
    fn config_state_valid_json_is_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("repo-config.json");
        fs::write(&path, r#"{ "repo_path": "/tmp/lib", "pending_migration_from": null }"#)
            .unwrap();
        match load_config_state_from(&path) {
            ConfigState::Valid(config) => {
                assert_eq!(config.repo_path.as_deref(), Some("/tmp/lib"));
            }
            other => panic!("expected Valid, got {other:?}"),
        }
    }

    #[test]
    fn config_state_corrupt_json_is_invalid_not_fresh_install() {
        // A corrupt config must never be treated like a missing one — that is
        // the "library rebuilt empty, all skills lost" failure mode (#228).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("repo-config.json");
        fs::write(&path, "{ not json").unwrap();
        let state = load_config_state_from(&path);
        assert!(matches!(state, ConfigState::Invalid(_)), "{state:?}");
    }

    #[test]
    fn external_base_dir_lives_under_default_base_external() {
        let dir = external_base_dir(Path::new("/tmp/some/my-skills"));
        let prefix = default_base_dir().join("external");
        assert!(
            dir.starts_with(&prefix),
            "expected {} to start with {}",
            dir.display(),
            prefix.display()
        );
    }

    #[test]
    fn external_base_dir_is_stable_for_same_path() {
        let a = external_base_dir(Path::new("/tmp/some/my-skills"));
        let b = external_base_dir(Path::new("/tmp/some/my-skills"));
        assert_eq!(a, b);
    }

    #[test]
    fn external_base_dir_differs_for_different_paths() {
        let a = external_base_dir(Path::new("/tmp/one/my-skills"));
        let b = external_base_dir(Path::new("/tmp/two/my-skills"));
        assert_ne!(a, b);
    }

    #[test]
    fn external_base_dir_does_not_pollute_skills_root_or_its_parent() {
        let skills_root = Path::new("/tmp/external-test/my-skills");
        let dir = external_base_dir(skills_root);
        assert!(!dir.starts_with(skills_root));
        assert!(!dir.starts_with(skills_root.parent().unwrap()));
    }

    #[test]
    fn sanitize_dir_name_replaces_unsafe_characters() {
        assert_eq!(sanitize_dir_name("my skills"), "my-skills");
        assert_eq!(sanitize_dir_name("a/b\\c:d"), "a-b-c-d");
        assert_eq!(sanitize_dir_name(""), "external");
    }

    #[test]
    fn external_base_dir_relative_path_is_stable_against_absolute_form() {
        // For a not-yet-existing target, a relative path should namespace the
        // same as its cwd-absolutized form. We simulate by passing both forms
        // and asserting they match.
        let cwd = std::env::current_dir().unwrap();
        let rel = Path::new("nonexistent-skills-target-xyz");
        let abs = cwd.join(rel);
        assert_eq!(external_base_dir(rel), external_base_dir(&abs));
    }

    #[test]
    fn external_base_dir_normalizes_redundant_segments() {
        // `./x`, `x`, and `a/../x` should all hash to the same namespace when
        // none of them exist on disk.
        let plain = external_base_dir(Path::new("nonexistent-norm-target"));
        let dot = external_base_dir(Path::new("./nonexistent-norm-target"));
        let parent = external_base_dir(Path::new("a/../nonexistent-norm-target"));
        assert_eq!(plain, dot);
        assert_eq!(plain, parent);
    }

    // ── storage layout split: internal App state vs configurable Library root ──

    fn config_with_repo_path(path: &Path) -> RepoPathConfig {
        RepoPathConfig {
            repo_path: Some(path.to_string_lossy().to_string()),
            pending_migration_from: None,
            ..RepoPathConfig::default()
        }
    }

    #[test]
    fn configured_external_library_keeps_app_state_internal() {
        let internal = Path::new("/internal/base");
        let external = Path::new("/Volumes/Ext/Library");

        let layout = resolve_layout(&config_with_repo_path(external), internal, None);

        assert_eq!(layout.state_base, internal);
        assert_eq!(layout.db_path(), internal.join("skills-manager.db"));
        assert_eq!(layout.scenarios_dir(), internal.join("scenarios"));
        assert_eq!(layout.cache_dir(), internal.join("cache"));
        assert_eq!(layout.logs_dir(), internal.join("logs"));
        // Only the Library content root follows the configured path.
        assert_eq!(layout.library_base, external);
        assert_eq!(layout.skills_dir(), external.join("skills"));
        assert!(layout.library_is_external());
    }

    #[test]
    fn default_config_keeps_library_and_state_internal() {
        let internal = Path::new("/internal/base");
        let layout = resolve_layout(&RepoPathConfig::default(), internal, None);

        assert_eq!(layout.state_base, internal);
        assert_eq!(layout.library_base, internal);
        assert_eq!(layout.skills_dir(), internal.join("skills"));
        assert_eq!(layout.db_path(), internal.join("skills-manager.db"));
        assert!(!layout.library_is_external());
    }

    #[test]
    fn cli_base_override_owns_both_state_and_library() {
        // CLI `--skills-root` / `--path` callers stay responsible for the root
        // they named, so an override keeps state and Library together even when
        // the shared config points somewhere else.
        let layout = resolve_layout(
            &config_with_repo_path(Path::new("/Volumes/Ext/Library")),
            Path::new("/internal/base"),
            Some(Path::new("/cli/root")),
        );

        assert_eq!(layout.state_base, Path::new("/cli/root"));
        assert_eq!(layout.library_base, Path::new("/cli/root"));
        assert_eq!(layout.skills_dir(), Path::new("/cli/root/skills"));
    }

    #[test]
    fn invalid_configured_path_falls_back_to_the_internal_library() {
        let config = RepoPathConfig {
            repo_path: Some("not/absolute".to_string()),
            pending_migration_from: None,
            ..RepoPathConfig::default()
        };
        let internal = Path::new("/internal/base");

        let layout = resolve_layout(&config, internal, None);

        assert_eq!(layout.library_base, internal);
        assert_eq!(layout.state_base, internal);
    }

    #[test]
    fn startup_dirs_never_include_an_external_library_root() {
        let internal = Path::new("/internal/base");
        let external = Path::new("/Volumes/Ext/Library");
        let layout = resolve_layout(&config_with_repo_path(external), internal, None);

        let dirs = startup_dirs_to_create(&layout);

        assert!(
            dirs.iter().all(|dir| !dir.starts_with(external)),
            "startup must never materialize an external Library root: {dirs:?}"
        );
        assert!(dirs.contains(&internal.join("scenarios")));
        assert!(dirs.contains(&internal.join("cache")));
        assert!(dirs.contains(&internal.join("logs")));
    }

    #[test]
    fn startup_dirs_include_the_internal_default_library() {
        let internal = Path::new("/internal/base");
        let layout = resolve_layout(&RepoPathConfig::default(), internal, None);

        assert!(startup_dirs_to_create(&layout).contains(&internal.join("skills")));
    }

    #[test]
    fn offline_external_library_still_yields_an_openable_internal_db() {
        let internal = tempfile::tempdir().unwrap();
        // Stands in for an unmounted external volume: configured, but absent.
        let missing_external = internal.path().join("not-mounted-volume");
        let layout = resolve_layout(
            &config_with_repo_path(&missing_external),
            internal.path(),
            None,
        );

        for dir in startup_dirs_to_create(&layout) {
            fs::create_dir_all(&dir).unwrap();
        }

        let store = crate::core::skill_store::SkillStore::new(&layout.db_path());
        assert!(
            store.is_ok(),
            "internal application state must open while the Library is offline"
        );
        assert!(
            !missing_external.exists(),
            "the configured Library path must not be created"
        );
    }

    // ── legacy v1 `repo_path` adoption (online path) ──

    use crate::core::skill_store::{ScenarioRecord, SkillRecord, SkillStore};

    fn seeded_legacy_base(skills: usize, scenarios: usize) -> tempfile::TempDir {
        let legacy = tempfile::tempdir().unwrap();
        fs::create_dir_all(legacy.path().join("skills")).unwrap();
        fs::create_dir_all(legacy.path().join("scenarios")).unwrap();
        fs::write(legacy.path().join("scenarios/startup.json"), b"{}").unwrap();

        let store = SkillStore::new(&legacy.path().join("skills-manager.db")).unwrap();
        for i in 0..skills {
            let id = format!("skill-{i}");
            let central = legacy.path().join("skills").join(&id);
            store
                .insert_skill(&SkillRecord {
                    id: id.clone(),
                    name: id.clone(),
                    description: None,
                    source_type: "import".to_string(),
                    source_ref: None,
                    source_ref_resolved: None,
                    source_subpath: None,
                    source_branch: None,
                    source_revision: None,
                    remote_revision: None,
                    central_path: central.to_string_lossy().to_string(),
                    content_hash: None,
                    enabled: true,
                    created_at: 1,
                    updated_at: 1,
                    status: "ok".to_string(),
                    update_status: "local_only".to_string(),
                    last_checked_at: None,
                    last_check_error: None,
                })
                .unwrap();
        }
        for i in 0..scenarios {
            store
                .insert_scenario(&ScenarioRecord {
                    id: format!("scenario-{i}"),
                    name: format!("scenario-{i}"),
                    description: None,
                    icon: None,
                    sort_order: i as i32,
                    created_at: 1,
                    updated_at: 1,
                })
                .unwrap();
        }
        drop(store);
        legacy
    }

    fn legacy_config(legacy: &Path) -> RepoPathConfig {
        RepoPathConfig {
            repo_path: Some(legacy.to_string_lossy().to_string()),
            ..RepoPathConfig::default()
        }
    }

    #[test]
    fn online_legacy_migration_copies_state_and_keeps_the_source() {
        let legacy = seeded_legacy_base(3, 2);
        let internal = tempfile::tempdir().unwrap();
        let mut config = legacy_config(legacy.path());

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(
            matches!(outcome, LegacyMigration::Completed),
            "expected Completed, got {outcome:?}"
        );

        // State landed on internal storage and still opens with the same rows.
        let store = SkillStore::new(&internal.path().join("skills-manager.db")).unwrap();
        assert_eq!(store.get_all_skills().unwrap().len(), 3);
        assert_eq!(store.get_all_scenarios().unwrap().len(), 2);
        assert!(internal.path().join("scenarios/startup.json").exists());

        // The Library now points at the legacy base; its files are untouched.
        assert_eq!(
            config.library_base.as_deref(),
            Some(legacy.path().to_string_lossy().as_ref())
        );
        assert_eq!(config.version, CONFIG_VERSION);
        assert!(legacy.path().join("skills-manager.db").exists());
        assert!(legacy.path().join("scenarios/startup.json").exists());
        assert!(legacy.path().join("skills").exists());

        // The adopted root carries an identity, and the config expects it.
        let recorded_id = config.library_id.clone().expect("library identity");
        let probe = crate::core::library_availability::probe_library(
            legacy.path(),
            Some(&recorded_id),
        );
        assert!(probe.is_online(), "the adopted Library must probe online");

        // The source stays recorded so a rollback can find it.
        let record = config.migration.as_ref().expect("migration record");
        assert_eq!(record.status, MigrationStatus::Completed);
        assert_eq!(record.legacy_source, legacy.path().to_string_lossy());

        let layout = resolve_layout(&config, internal.path(), None);
        assert_eq!(layout.state_base, internal.path());
        assert_eq!(layout.library_base, legacy.path());
    }

    #[test]
    fn legacy_migration_leaves_config_v1_when_verification_fails() {
        // A source DB that is not valid SQLite must never be adopted: the config
        // marker only advances after the copy is verified.
        let legacy = tempfile::tempdir().unwrap();
        fs::create_dir_all(legacy.path().join("skills")).unwrap();
        fs::write(legacy.path().join("skills-manager.db"), b"not a database").unwrap();
        let internal = tempfile::tempdir().unwrap();
        let mut config = legacy_config(legacy.path());

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(
            matches!(outcome, LegacyMigration::Failed(_)),
            "expected Failed, got {outcome:?}"
        );
        assert_eq!(config.version, 0, "config must stay on v1 for a retry");
        assert_eq!(config.library_base, None);
        assert!(config.migration.is_none());
        assert!(
            !internal.path().join("skills-manager.db").exists(),
            "an unverified copy must not be left behind"
        );
        assert!(
            !internal.path().join(MIGRATION_STAGING_DIR).exists(),
            "staging must be cleaned up"
        );
        assert_eq!(fs::read(legacy.path().join("skills-manager.db")).unwrap(), b"not a database");
    }

    #[test]
    fn default_internal_layout_is_stamped_without_migrating() {
        let internal = tempfile::tempdir().unwrap();
        let mut config = RepoPathConfig::default();

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(matches!(outcome, LegacyMigration::NotNeeded));
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.library_base, None);
        assert!(config.migration.is_none());
    }

    #[test]
    fn already_migrated_config_is_left_alone() {
        let legacy = seeded_legacy_base(1, 1);
        let internal = tempfile::tempdir().unwrap();
        let mut config = RepoPathConfig {
            version: CONFIG_VERSION,
            repo_path: Some(legacy.path().to_string_lossy().to_string()),
            library_base: Some(legacy.path().to_string_lossy().to_string()),
            ..RepoPathConfig::default()
        };
        let before = config.clone();

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(matches!(outcome, LegacyMigration::NotNeeded));
        assert_eq!(config.library_base, before.library_base);
        assert!(
            !internal.path().join("skills-manager.db").exists(),
            "a completed migration must not copy again"
        );
    }

    // ── legacy v1 adoption: offline and conflict branches ──

    /// Content fingerprint of a directory tree: relative path + bytes for every
    /// file. Used to prove a rejected migration touched nothing.
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

    #[test]
    fn offline_legacy_volume_defers_migration_and_keeps_retrying() {
        let internal = tempfile::tempdir().unwrap();
        // A configured path whose volume is not mounted.
        let missing = internal.path().join("Volumes/Ext/Library");
        let mut config = legacy_config(&missing);
        let before = tree_hash(internal.path());

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(
            matches!(outcome, LegacyMigration::PendingOffline),
            "expected PendingOffline, got {outcome:?}"
        );
        assert_eq!(config.version, 0, "the upgrade must retry on a later launch");
        assert_eq!(config.library_base, None);
        assert_eq!(
            config.repo_path.as_deref(),
            Some(missing.to_string_lossy().as_ref()),
            "the configured path stays recorded for the retry"
        );
        assert!(!missing.exists(), "an absent volume must not be created");
        assert_eq!(
            tree_hash(internal.path()),
            before,
            "internal state must be untouched"
        );
    }

    #[test]
    fn conflicting_state_on_both_sides_blocks_migration() {
        let legacy = seeded_legacy_base(2, 1);
        let internal = tempfile::tempdir().unwrap();
        // Internal storage already holds its own database.
        let internal_store = SkillStore::new(&internal.path().join("skills-manager.db")).unwrap();
        internal_store
            .insert_scenario(&ScenarioRecord {
                id: "internal-only".to_string(),
                name: "internal-only".to_string(),
                description: None,
                icon: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        drop(internal_store);

        let mut config = legacy_config(legacy.path());
        let legacy_before = tree_hash(legacy.path());
        let internal_before = tree_hash(internal.path());

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(
            matches!(outcome, LegacyMigration::Blocked(_)),
            "expected Blocked, got {outcome:?}"
        );
        assert_eq!(
            tree_hash(legacy.path()),
            legacy_before,
            "the legacy location must not be overwritten or deleted"
        );
        assert_eq!(
            tree_hash(internal.path()),
            internal_before,
            "internal state must not be overwritten or deleted"
        );
        assert_eq!(config.library_base, None, "no side is silently adopted");
        assert_eq!(config.version, 0);

        // Row counts on both sides survive unchanged.
        let legacy_store = SkillStore::new(&legacy.path().join("skills-manager.db")).unwrap();
        assert_eq!(legacy_store.get_all_skills().unwrap().len(), 2);
        let internal_store = SkillStore::new(&internal.path().join("skills-manager.db")).unwrap();
        assert_eq!(internal_store.get_all_skills().unwrap().len(), 0);
        assert_eq!(internal_store.get_all_scenarios().unwrap().len(), 1);
    }

    #[test]
    fn blocked_migration_records_a_retryable_marker() {
        // The marker exists so Settings can explain the state and the next
        // launch re-evaluates instead of silently picking a side.
        let legacy = seeded_legacy_base(1, 0);
        let internal = tempfile::tempdir().unwrap();
        SkillStore::new(&internal.path().join("skills-manager.db")).unwrap();
        let mut config = legacy_config(legacy.path());

        migrate_legacy_layout(&mut config, internal.path());

        let record = config.migration.as_ref().expect("migration record");
        assert_eq!(record.status, MigrationStatus::Blocked);
        assert_eq!(record.legacy_source, legacy.path().to_string_lossy());
        assert_eq!(config.version, 0, "still v1, so the next launch retries");
    }

    #[test]
    fn offline_migration_records_a_retryable_marker() {
        let internal = tempfile::tempdir().unwrap();
        let missing = internal.path().join("not-mounted");
        let mut config = legacy_config(&missing);

        migrate_legacy_layout(&mut config, internal.path());

        let record = config.migration.as_ref().expect("migration record");
        assert_eq!(record.status, MigrationStatus::PendingOffline);
        assert_eq!(record.legacy_source, missing.to_string_lossy());
    }

    #[test]
    fn a_previously_blocked_migration_completes_once_the_conflict_is_gone() {
        let legacy = seeded_legacy_base(2, 1);
        let internal = tempfile::tempdir().unwrap();
        let mut config = legacy_config(legacy.path());
        config.migration = Some(MigrationRecord {
            legacy_source: legacy.path().to_string_lossy().to_string(),
            status: MigrationStatus::Blocked,
        });

        let outcome = migrate_legacy_layout(&mut config, internal.path());

        assert!(
            matches!(outcome, LegacyMigration::Completed),
            "expected Completed, got {outcome:?}"
        );
        assert_eq!(
            config.migration.as_ref().unwrap().status,
            MigrationStatus::Completed
        );
        assert_eq!(config.version, CONFIG_VERSION);
    }

    #[test]
    fn migrated_config_prefers_library_base_over_legacy_repo_path() {
        let internal = Path::new("/internal/base");
        let config = RepoPathConfig {
            version: CONFIG_VERSION,
            repo_path: Some("/old/legacy/base".to_string()),
            library_base: Some("/Volumes/Ext/Library".to_string()),
            ..RepoPathConfig::default()
        };

        let layout = resolve_layout(&config, internal, None);

        assert_eq!(layout.library_base, Path::new("/Volumes/Ext/Library"));
        assert_eq!(layout.state_base, internal);
    }

    #[test]
    fn migrated_config_without_library_base_uses_the_internal_library() {
        let internal = Path::new("/internal/base");
        let config = RepoPathConfig {
            version: CONFIG_VERSION,
            repo_path: Some("/old/legacy/base".to_string()),
            library_base: None,
            ..RepoPathConfig::default()
        };

        let layout = resolve_layout(&config, internal, None);

        assert_eq!(layout.library_base, internal);
    }

    #[test]
    fn lexically_normalize_handles_basic_cases() {
        assert_eq!(
            lexically_normalize(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            lexically_normalize(Path::new("./a/b")),
            PathBuf::from("a/b")
        );
        assert_eq!(lexically_normalize(Path::new("/..")), PathBuf::from("/"));
    }
}
