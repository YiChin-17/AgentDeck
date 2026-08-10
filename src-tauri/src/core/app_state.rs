use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use super::{
    central_repo, library_availability, scenario_service, skill_store::SkillStore, sync_metadata,
    tool_service,
};

/// Per-stage timings collected during `initialize_store`. The struct is
/// returned to the caller so the log lines can be emitted once
/// `tauri_plugin_log` is registered — anything logged from inside this
/// function would otherwise be dropped because the logger isn't installed
/// until later in `tauri::Builder::setup`. See issue #153.
#[derive(Debug, Clone)]
pub struct StartupTimings {
    pub ensure_central_repo_ms: u128,
    pub open_store_ms: u128,
    pub migrate_legacy_tool_keys_ms: u128,
    pub skill_count: usize,
    pub reindex_from_metadata_ms: Option<u128>,
    pub restore_sync_included_ms: u128,
    pub restore_sync_included_changed: bool,
    pub write_all_from_db_ms: Option<u128>,
    pub apply_scenario_ms: u128,
    /// "default_startup" (Tauri app) or "cli" (CLI bin). Defaults to
    /// `"unknown"` so a struct that escapes `initialize_store_inner`
    /// without being fully populated still produces an obvious value in
    /// the log instead of an empty string.
    pub apply_scenario_kind: &'static str,
    /// Whether the Library was verified at startup. False means the Library
    /// steps above were deliberately skipped.
    pub library_online: bool,
    pub total_ms: u128,
}

impl Default for StartupTimings {
    fn default() -> Self {
        Self {
            ensure_central_repo_ms: 0,
            open_store_ms: 0,
            migrate_legacy_tool_keys_ms: 0,
            skill_count: 0,
            reindex_from_metadata_ms: None,
            restore_sync_included_ms: 0,
            restore_sync_included_changed: false,
            write_all_from_db_ms: None,
            apply_scenario_ms: 0,
            apply_scenario_kind: "unknown",
            library_online: true,
            total_ms: 0,
        }
    }
}

pub fn initialize_store() -> Result<(Arc<SkillStore>, StartupTimings)> {
    initialize_store_inner(true)
}

pub fn initialize_cli_store() -> Result<Arc<SkillStore>> {
    initialize_store_inner(false).map(|(store, _)| store)
}

fn initialize_store_inner(
    apply_startup_default: bool,
) -> Result<(Arc<SkillStore>, StartupTimings)> {
    // Decide availability once, before any step that reads the Library. An
    // offline Library must not be reindexed or synced: its absent files would
    // be recorded as deletions.
    let library_online = library_availability::refresh_availability().is_online();
    initialize_store_with(apply_startup_default, library_online)
}

/// Startup with the availability verdict supplied, so the offline path can be
/// exercised without an external volume.
fn initialize_store_with(
    apply_startup_default: bool,
    library_online: bool,
) -> Result<(Arc<SkillStore>, StartupTimings)> {
    let total_start = Instant::now();
    let mut timings = StartupTimings::default();
    timings.library_online = library_online;

    let step = Instant::now();
    central_repo::ensure_central_repo().context("Failed to create central repo")?;
    timings.ensure_central_repo_ms = step.elapsed().as_millis();

    let db_path = central_repo::db_path();
    let step = Instant::now();
    let store = Arc::new(SkillStore::new(&db_path).context("Failed to initialize database")?);
    timings.open_store_ms = step.elapsed().as_millis();

    let step = Instant::now();
    tool_service::migrate_legacy_tool_keys(&store)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("Failed to migrate legacy tool keys")?;
    timings.migrate_legacy_tool_keys_ms = step.elapsed().as_millis();

    timings.skill_count = store.get_all_skills().map(|s| s.len()).unwrap_or(0);

    if library_online && sync_metadata::metadata_exists() {
        let step = Instant::now();
        sync_metadata::reindex_from_metadata(&store)
            .context("Failed to reindex from sync metadata")?;
        timings.reindex_from_metadata_ms = Some(step.elapsed().as_millis());
    }

    let step = Instant::now();
    let changed = if library_online {
        scenario_service::restore_all_skills_sync_included(&store)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .context("Failed to restore skill sync inclusion")?
    } else {
        false
    };
    timings.restore_sync_included_ms = step.elapsed().as_millis();
    timings.restore_sync_included_changed = changed;
    if changed {
        let step = Instant::now();
        sync_metadata::write_all_from_db(&store)
            .context("Failed to persist restored skill sync inclusion")?;
        timings.write_all_from_db_ms = Some(step.elapsed().as_millis());
    }

    let step = Instant::now();
    if !library_online {
        // Applying a scenario writes deployment targets from Library files that
        // are not there. The app still starts; the UI shows the offline state.
        timings.apply_scenario_kind = "skipped_offline";
    } else if apply_startup_default {
        scenario_service::ensure_default_startup_scenario(&store)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .context("Failed to initialize startup scenario")?;
        timings.apply_scenario_kind = "default_startup";
    } else {
        scenario_service::ensure_cli_scenario_state(&store)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .context("Failed to initialize CLI scenario state")?;
        timings.apply_scenario_kind = "cli";
    }
    timings.apply_scenario_ms = step.elapsed().as_millis();

    timings.total_ms = total_start.elapsed().as_millis();
    Ok((store, timings))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Startup against an offline Library must open the internal database and
    /// stop there. Reindexing or applying a scenario would read the absent
    /// files as deletions and write that conclusion into the database and the
    /// deployment targets.
    #[test]
    fn offline_startup_opens_the_database_and_skips_library_work() {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().join("state")));

        // Metadata that an online startup would reindex from.
        let metadata = sync_metadata::metadata_dir();
        std::fs::create_dir_all(metadata.join("skills")).unwrap();
        std::fs::write(metadata.join("schema.json"), b"{}").unwrap();
        assert!(sync_metadata::metadata_exists());

        let (store, timings) = initialize_store_with(true, false).expect("app must still start");

        central_repo::set_test_base_dir_override(None);

        assert!(store.get_all_skills().is_ok(), "internal database is usable");
        assert_eq!(
            timings.reindex_from_metadata_ms, None,
            "an offline Library must not be reindexed"
        );
        assert!(
            !timings.restore_sync_included_changed,
            "sync inclusion must not be restored from an unavailable Library"
        );
        assert_eq!(timings.apply_scenario_kind, "skipped_offline");
        assert!(!timings.library_online);
    }

    /// The same startup online still does the Library work, so the guard above
    /// cannot silently disable it for everyone.
    #[test]
    fn online_startup_still_applies_the_startup_scenario() {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().join("state")));

        let (_store, timings) = initialize_store_with(true, true).expect("app must start");

        central_repo::set_test_base_dir_override(None);

        assert_eq!(timings.apply_scenario_kind, "default_startup");
        assert!(timings.library_online);
    }
}

impl StartupTimings {
    /// Emit a single human-readable log block from the captured timings.
    /// Called from `tauri::Builder::setup` once `tauri_plugin_log` is
    /// installed; calling it before that point would lose the output to
    /// the no-op default logger.
    pub fn log(&self) {
        log::info!(
            "startup: initialize_store total {} ms (skills={})",
            self.total_ms,
            self.skill_count
        );
        log::info!(
            "startup: ensure_central_repo {} ms, open_store {} ms, migrate_legacy_tool_keys {} ms",
            self.ensure_central_repo_ms,
            self.open_store_ms,
            self.migrate_legacy_tool_keys_ms
        );
        if let Some(ms) = self.reindex_from_metadata_ms {
            log::info!(
                "startup: reindex_from_metadata {} ms (skills={})",
                ms,
                self.skill_count
            );
        }
        if self.restore_sync_included_changed {
            log::info!(
                "startup: restore_sync_included changed in {} ms, write_all_from_db {} ms",
                self.restore_sync_included_ms,
                self.write_all_from_db_ms.unwrap_or(0)
            );
        } else {
            log::info!(
                "startup: restore_sync_included no-op in {} ms",
                self.restore_sync_included_ms
            );
        }
        log::info!(
            "startup: apply_scenario ({}) {} ms (skills={})",
            self.apply_scenario_kind,
            self.apply_scenario_ms,
            self.skill_count
        );
    }
}
