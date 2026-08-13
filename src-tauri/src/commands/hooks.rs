//! Hook inspection and gated Hook editing commands.
//!
//! No Hook is ever executed here, and nothing from a Hook source is logged or
//! stored: every command takes a Project id and an enum-backed source id, never
//! a filesystem path, and all writing happens in `core::hook_management`.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::core::central_repo;
use crate::core::error::{AppError, ErrorKind};
use crate::core::hook_inspection::{self, HookInspectionDto};
use crate::core::hook_management::{
    self, HookApplyOutcome, HookEditOperationDto, HookRecoveryDto, HookWriteEnv, HookWriteError,
    HookWritePreviewDto,
};
use crate::core::skill_store::SkillStore;

/// Resolves the optional Project id and inspects the fixed sources.
///
/// `lookup` returns a linked Project's root path. It is a parameter so the
/// boundary can be tested without a database, and so the command can never turn
/// a caller-supplied string into a path on its own.
fn build_inspection(
    home: Option<PathBuf>,
    project_id: Option<String>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<HookInspectionDto, AppError> {
    let project_root = hook_inspection::resolve_project_root(project_id.as_deref(), lookup)
        .map_err(|err| AppError {
            kind: ErrorKind::InvalidInput,
            message: err.as_str().to_string(),
        })?;

    Ok(hook_inspection::inspect(
        home.as_deref(),
        project_root.as_deref(),
        project_id,
    ))
}

/// Returns the read-only Hook inspection for the user scope, plus the given
/// linked Project's scope when one is selected.
#[tauri::command]
pub async fn get_hook_inspection(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
) -> Result<HookInspectionDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_inspection(dirs::home_dir(), project_id, |id| {
            store
                .get_project_by_id(id)
                .ok()
                .flatten()
                .map(|record| record.path)
        })
    })
    .await?
}

/// Where owner-private recovery payloads live: application state, never the
/// Library Git tree that gets synced and backed up.
fn backup_root() -> PathBuf {
    central_repo::base_dir().join("hook-backups")
}

/// Every Hook write failure reaches the frontend as one stable code.
///
/// The code is the whole message: a parser or filesystem string could carry a
/// path or a fragment of the user's configuration.
fn write_error(error: HookWriteError) -> AppError {
    AppError {
        kind: match error {
            HookWriteError::InvalidProject
            | HookWriteError::InvalidSource
            | HookWriteError::InvalidHookDraft
            | HookWriteError::StaleDraft
            | HookWriteError::UnsupportedSourceType
            | HookWriteError::PreviewTooLarge => ErrorKind::InvalidInput,
            HookWriteError::SourceOffline => ErrorKind::NotFound,
            HookWriteError::SourceConflict | HookWriteError::RecoveryRequired => {
                ErrorKind::Cancelled
            }
            HookWriteError::BackupFailed
            | HookWriteError::AtomicReplaceUnsupported
            | HookWriteError::WriteFailed
            | HookWriteError::RestoreFailed => ErrorKind::Io,
        },
        message: error.as_str().to_string(),
    }
}

/// Runs `action` with an environment built from application state.
fn with_env<T>(
    store: Arc<SkillStore>,
    action: impl FnOnce(&HookWriteEnv<'_>) -> Result<T, HookWriteError>,
) -> Result<T, AppError> {
    let home = dirs::home_dir();
    let root = backup_root();
    let env = HookWriteEnv {
        home: home.as_deref(),
        backup_root: &root,
        store: &store,
        fault: None,
    };
    action(&env).map_err(write_error)
}

/// Returns the diff, base revision and validation result of a draft.
///
/// Nothing is written: the preview path is given no backup root and no database
/// handle, so it cannot persist even by mistake.
#[tauri::command]
pub async fn preview_hook_change(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
    source_id: String,
    operations: Vec<HookEditOperationDto>,
) -> Result<HookWritePreviewDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        hook_management::ensure_mutations_allowed(&backup_root()).map_err(write_error)?;
        hook_management::preview_change(
            dirs::home_dir().as_deref(),
            project_id.as_deref(),
            &source_id,
            &operations,
            |id| {
                store
                    .get_project_by_id(id)
                    .ok()
                    .flatten()
                    .map(|record| record.path)
            },
        )
        .map_err(write_error)
    })
    .await?
}

/// Applies a previewed draft against the exact revision it was built from.
#[tauri::command]
pub async fn apply_hook_change(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
    source_id: String,
    base_revision: String,
    operations: Vec<HookEditOperationDto>,
) -> Result<HookApplyOutcome, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_env(store, |env| {
            hook_management::apply_change(
                env,
                project_id.as_deref(),
                &source_id,
                &base_revision,
                &operations,
            )
        })
    })
    .await?
}

/// Returns the latest recovery point of a managed source, without its payload.
#[tauri::command]
pub async fn get_hook_recovery(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
    source_id: String,
) -> Result<Option<HookRecoveryDto>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_env(store, |env| {
            hook_management::get_recovery(env, project_id.as_deref(), &source_id)
        })
    })
    .await?
}

/// Diffs the current source against the recovery point it would be restored to.
#[tauri::command]
pub async fn preview_hook_restore(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
    source_id: String,
    backup_id: String,
) -> Result<HookWritePreviewDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_env(store, |env| {
            hook_management::ensure_mutations_allowed(env.backup_root)?;
            hook_management::preview_restore(env, project_id.as_deref(), &source_id, &backup_id)
        })
    })
    .await?
}

/// Restores the recovery point and leaves the pre-restore state as the new one.
#[tauri::command]
pub async fn apply_hook_restore(
    store: State<'_, Arc<SkillStore>>,
    project_id: Option<String>,
    source_id: String,
    backup_id: String,
    base_revision: String,
) -> Result<HookApplyOutcome, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_env(store, |env| {
            hook_management::apply_restore(
                env,
                project_id.as_deref(),
                &source_id,
                &backup_id,
                &base_revision,
            )
        })
    })
    .await?
}

/// Brings any interrupted Hook write back to a consistent state.
///
/// Called once at startup, before the UI can request a mutation; a failure
/// leaves inspection working while every mutation reports `recovery_required`.
pub fn reconcile_hook_writes(store: &Arc<SkillStore>) -> Result<(), AppError> {
    with_env(store.clone(), hook_management::reconcile_pending_operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use crate::core::error::ErrorKind;
    use crate::core::hook_inspection::HookSourceStatus;

    fn write_fixture(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture dir");
        fs::write(path, content).expect("write fixture");
    }

    const CLAUDE_SETTINGS: &str = r#"{
  "apiToken": "sentinel-secret",
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo ok" } ] }
    ]
  }
}"#;

    #[test]
    fn unknown_project_returns_a_typed_invalid_project_error() {
        let home = tempfile::tempdir().expect("home");
        let error = build_inspection(Some(home.path().to_path_buf()), Some("nope".to_string()), |_| {
            None
        })
        .expect_err("unknown project must fail");

        assert_eq!(error.kind, ErrorKind::InvalidInput);
        assert_eq!(error.message, "invalid_project");
    }

    #[test]
    fn linked_project_is_resolved_through_the_store_lookup() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        let root = project.path().to_path_buf();

        let dto = build_inspection(
            Some(home.path().to_path_buf()),
            Some("project-1".to_string()),
            move |id| {
                if id == "project-1" {
                    Some(root.to_string_lossy().to_string())
                } else {
                    None
                }
            },
        )
        .expect("inspection");

        assert_eq!(dto.sources.len(), 7);
        assert_eq!(dto.selected_project_id.as_deref(), Some("project-1"));
    }

    /// The frontend branches on these strings, so they are part of the contract.
    #[test]
    fn serialized_response_uses_the_fixed_vocabulary() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            CLAUDE_SETTINGS,
        );
        write_fixture(&home.path().join(".codex").join("hooks.json"), "{ broken");
        write_fixture(
            &home.path().join(".codex").join("config.toml"),
            "[[hooks.SessionStart]]\nmatcher = \"startup\"\n\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = \"echo start\"\n",
        );
        // Oversized project source: valid JSON, over the 1 MiB read limit.
        write_fixture(
            &project.path().join(".claude").join("settings.json"),
            &format!(r#"{{"hooks":{{}},"pad":"{}"}}"#, "x".repeat(1_048_600)),
        );

        let root = project.path().to_path_buf();
        let dto = build_inspection(
            Some(home.path().to_path_buf()),
            Some("project-1".to_string()),
            move |_| Some(root.to_string_lossy().to_string()),
        )
        .expect("inspection");

        let status_of = |id: &str| {
            dto.sources
                .iter()
                .find(|s| s.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
                .status
        };
        assert_eq!(status_of("claude_code:user:settings-json"), HookSourceStatus::Valid);
        assert_eq!(status_of("codex:user:hooks-json"), HookSourceStatus::Invalid);
        assert_eq!(status_of("codex:user:config-toml"), HookSourceStatus::Valid);
        assert_eq!(
            status_of("claude_code:project:settings-json"),
            HookSourceStatus::TooLarge
        );
        assert_eq!(
            status_of("codex:project:hooks-json"),
            HookSourceStatus::Missing
        );

        let json = serde_json::to_value(&dto).expect("serialize");
        let sources = json["sources"].as_array().expect("sources array");
        let by_id = |id: &str| {
            sources
                .iter()
                .find(|s| s["id"] == id)
                .unwrap_or_else(|| panic!("{id} missing"))
        };

        assert_eq!(by_id("codex:user:hooks-json")["agent"], "codex");
        assert_eq!(by_id("codex:user:hooks-json")["scope"], "user");
        assert_eq!(by_id("codex:user:hooks-json")["format"], "json");
        assert_eq!(by_id("codex:user:hooks-json")["status"], "invalid");
        assert_eq!(
            by_id("codex:user:hooks-json")["diagnostic"]["kind"],
            "invalid_syntax"
        );
        assert_eq!(by_id("codex:user:config-toml")["format"], "toml");
        assert_eq!(
            by_id("claude_code:user:settings-json")["agent"],
            "claude_code"
        );
        assert_eq!(
            by_id("claude_code:project_local:settings-local-json")["scope"],
            "project_local"
        );
        assert_eq!(
            by_id("claude_code:project:settings-json")["status"],
            "too_large"
        );
        assert_eq!(by_id("codex:project:hooks-json")["status"], "missing");

        let entry = json["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .find(|e| e["sourceId"] == "claude_code:user:settings-json")
            .expect("Claude Code entry");
        assert_eq!(entry["event"], "PreToolUse");
        assert_eq!(entry["eventKnown"], true);
        assert_eq!(entry["handlerType"], "command");
        assert_eq!(entry["handlerTypeKnown"], true);
        assert_eq!(entry["fields"][0]["key"], "command");

        // The non-Hook sibling that sits next to `hooks` never leaves the backend.
        let serialized = serde_json::to_string(&dto).expect("serialize");
        assert!(!serialized.contains("sentinel-secret"));
        assert!(!serialized.contains("apiToken"));

        assert_eq!(json["snapshotDate"], "2026-08-12");
        assert_eq!(json["compatibility"][0]["category"], "event");
        assert!(json["generatedAt"].as_i64().expect("generatedAt") > 0);
        assert!(json["sources"][0]["diffAvailable"].is_boolean());
    }

    // Requirement: Inspection responses exclude non-Hook configuration and persistence
    // Scenario: Read-only inspection has no side effects
    #[test]
    fn inspection_changes_no_file_and_no_database_row() {
        use crate::core::skill_store::{ProjectRecord, SkillStore};

        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        let db_dir = tempfile::tempdir().expect("db");

        let settings = home.path().join(".claude").join("settings.json");
        let config = home.path().join(".codex").join("config.toml");
        let project_settings = project.path().join(".claude").join("settings.local.json");
        write_fixture(&settings, CLAUDE_SETTINGS);
        write_fixture(&config, "[[hooks.SessionStart]]\nmatcher = \"startup\"\n\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = \"echo start\"\n");
        write_fixture(&project_settings, CLAUDE_SETTINGS);

        let store = SkillStore::new(&db_dir.path().join("test.db")).expect("store");
        store
            .insert_project(&ProjectRecord {
                id: "project-1".to_string(),
                name: "Demo".to_string(),
                path: project.path().to_string_lossy().to_string(),
                workspace_type: "project".to_string(),
                linked_agent_key: None,
                linked_agent_name: None,
                disabled_path: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .expect("insert project");

        // A whole-tree snapshot also catches a file that appears or disappears,
        // which a per-file byte comparison would miss.
        let snapshot = |root: &Path| -> Vec<(String, Vec<u8>)> {
            let mut items: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| {
                    (
                        entry
                            .path()
                            .strip_prefix(root)
                            .expect("relative path")
                            .to_string_lossy()
                            .to_string(),
                        fs::read(entry.path()).expect("read tree file"),
                    )
                })
                .collect();
            items.sort();
            items
        };

        let fixtures = [&settings, &config, &project_settings];
        let bytes_before: Vec<Vec<u8>> = fixtures.iter().map(|p| fs::read(p).expect("read")).collect();
        let tree_before = (snapshot(home.path()), snapshot(project.path()));
        let rows_before = format!("{:?}", store.get_all_projects().expect("projects"));

        let dto = build_inspection(
            Some(home.path().to_path_buf()),
            Some("project-1".to_string()),
            |id| {
                store
                    .get_project_by_id(id)
                    .ok()
                    .flatten()
                    .map(|record| record.path)
            },
        )
        .expect("inspection");
        assert!(!dto.entries.is_empty());

        let bytes_after: Vec<Vec<u8>> = fixtures.iter().map(|p| fs::read(p).expect("read")).collect();
        let tree_after = (snapshot(home.path()), snapshot(project.path()));
        let rows_after = format!("{:?}", store.get_all_projects().expect("projects"));

        assert_eq!(bytes_before, bytes_after, "source files must be untouched");
        assert_eq!(tree_before, tree_after, "no file may be added or removed");
        assert_eq!(rows_before, rows_after, "no row may be written");
    }

    /// The frontend branches on these strings and localizes them, so every one
    /// of them is part of the contract.
    #[test]
    fn hook_write_errors_serialize_to_stable_keys() {
        let expected = [
            (HookWriteError::InvalidProject, "invalid_project"),
            (HookWriteError::InvalidSource, "invalid_source"),
            (HookWriteError::SourceOffline, "source_offline"),
            (
                HookWriteError::UnsupportedSourceType,
                "unsupported_source_type",
            ),
            (HookWriteError::InvalidHookDraft, "invalid_hook_draft"),
            (HookWriteError::StaleDraft, "stale_draft"),
            (HookWriteError::SourceConflict, "source_conflict"),
            (HookWriteError::PreviewTooLarge, "preview_too_large"),
            (HookWriteError::BackupFailed, "backup_failed"),
            (
                HookWriteError::AtomicReplaceUnsupported,
                "atomic_replace_unsupported",
            ),
            (HookWriteError::WriteFailed, "write_failed"),
            (HookWriteError::RestoreFailed, "restore_failed"),
            (HookWriteError::RecoveryRequired, "recovery_required"),
        ];

        for (error, key) in expected {
            let app_error = write_error(error);
            assert_eq!(app_error.message, key);
            let json = serde_json::to_value(&app_error).expect("serialize");
            assert_eq!(json["message"], key, "{key} must reach the frontend as-is");
        }
    }

    /// A request only ever names a source and a handler position — there is no
    /// field a caller could put a filesystem path in.
    #[test]
    fn hook_edit_operations_deserialize_from_the_frontend_shape() {
        let operations: Vec<HookEditOperationDto> = serde_json::from_str(
            r#"[
              {"kind":"create_handler","draft":{"event":"PreToolUse","matcher":"Bash",
               "handlerType":"command","fields":{"command":"echo hi"}}},
              {"kind":"update_handler","locator":{"event":"PreToolUse","groupIndex":0,"handlerIndex":1},
               "draft":{"event":"PreToolUse","matcher":null,"handlerType":"command","fields":{}}},
              {"kind":"delete_handler","locator":{"event":"Stop","groupIndex":2,"handlerIndex":0}}
            ]"#,
        )
        .expect("frontend request shape");

        assert_eq!(operations.len(), 3);
        let serialized = serde_json::to_string(&operations).expect("serialize");
        assert!(serialized.contains("\"groupIndex\""));
        assert!(serialized.contains("\"handlerType\""));
        assert!(!serialized.contains("path"));
    }

    /// Hook commands can carry credentials, so no part of the write path may
    /// print or log. `hook_management` is allowed to write files — that is its
    /// job — but not to emit anything.
    #[test]
    fn hook_write_source_has_no_log_call() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("core")
            .join("hook_management.rs");
        let text = fs::read_to_string(&file).expect("read production source");
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text).to_string();
        for needle in [
            "log::", "println!", "eprintln!", "info!", "warn!", "error!", "debug!", "trace!",
            "Command::new",
        ] {
            assert!(
                !production.contains(needle),
                "hook_management.rs must not contain {needle}"
            );
        }
    }

    /// Hook commands can carry credentials, so the read path must not have a
    /// way to emit or store them. This is asserted against the production
    /// source: a logging call added later would compile fine and only be caught
    /// by a person reading a log file.
    #[test]
    fn inspection_source_has_no_log_or_write_call() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = [
            src.join("commands").join("hooks.rs"),
            src.join("core").join("hook_inspection.rs"),
        ];
        let forbidden = [
            "log::",
            "println!",
            "eprintln!",
            "info!",
            "warn!",
            "error!",
            "debug!",
            "trace!",
            "fs::write",
            "fs::create",
            "File::create",
            "OpenOptions",
            "execute(",
            "Command::new",
        ];

        for file in files {
            let text = fs::read_to_string(&file).expect("read production source");
            // Only the half above the first test module is production code.
            let production = text.split("#[cfg(test)]").next().unwrap_or(&text).to_string();
            for needle in forbidden {
                assert!(
                    !production.contains(needle),
                    "{} must not contain {needle}",
                    file.display()
                );
            }
        }
    }
}
