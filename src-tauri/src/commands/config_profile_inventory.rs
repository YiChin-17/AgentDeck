//! Read-only Config Profile inventory command.
//!
//! The request carries an optional registered Project id and two filters. It
//! cannot carry a path, a working directory or an environment: the request type
//! denies unknown fields, and every path is composed in
//! `core::config_profile_inventory` from the fixed table plus the Project root
//! that the store — not the caller — supplies.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use crate::core::config_profile_inventory::{
    self, ConfigInventoryError, ConfigInventoryRequest, ConfigProfileInventoryDto,
};
use crate::core::error::{AppError, ErrorKind};
use crate::core::skill_store::SkillStore;

/// Every request failure reaches the frontend as one stable code.
fn request_error(error: ConfigInventoryError) -> AppError {
    AppError {
        kind: match error {
            ConfigInventoryError::ProjectNotFound => ErrorKind::NotFound,
        },
        message: error.as_str().to_string(),
    }
}

/// Resolves the request against a Project lookup and inspects the fixed sources.
///
/// `lookup` is a parameter so the boundary can be exercised without a database,
/// and so the command can never turn a caller-supplied string into a path.
fn build(
    home: Option<&Path>,
    request: &ConfigInventoryRequest,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<ConfigProfileInventoryDto, AppError> {
    config_profile_inventory::build_inventory(home, request, Utc::now().timestamp(), lookup)
        .map_err(request_error)
}

/// Returns the read-only Config Profile inventory for the user scope, plus the
/// given registered Project's scopes when one is selected.
#[tauri::command]
pub async fn get_config_profile_inventory(
    store: State<'_, Arc<SkillStore>>,
    request: ConfigInventoryRequest,
) -> Result<ConfigProfileInventoryDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build(dirs::home_dir().as_deref(), &request, |id| {
            store.get_project_by_id(id).ok().flatten().map(|record| record.path)
        })
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config_profile_inventory::{
        ConfigAgent, ConfigScope, ConfigSourceStatus,
    };
    use std::fs;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("path has a parent")).expect("create parent");
        fs::write(path, contents).expect("write source");
    }

    /// A temporary home and project seeded with one source per fixed location.
    fn seeded() -> (tempfile::TempDir, tempfile::TempDir) {
        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"sonnet"}"#);
        write(&project.path().join(".codex").join("config.toml"), "model = \"gpt-5-codex\"\n");
        write(&project.path().join(".claude").join("settings.json"), r#"{"model":"opus"}"#);
        write(
            &project.path().join(".claude").join("settings.local.json"),
            r#"{"model":"haiku"}"#,
        );
        (home, project)
    }

    fn request(
        project_id: Option<&str>,
        agent: Option<ConfigAgent>,
        scope: Option<ConfigScope>,
    ) -> ConfigInventoryRequest {
        ConfigInventoryRequest { project_id: project_id.map(str::to_string), agent, scope }
    }

    #[test]
    fn config_profile_inventory_unfiltered_request_returns_all_five_sources() {
        let (home, project) = seeded();
        let root = project.path().to_string_lossy().to_string();

        let inventory = build(Some(home.path()), &request(Some("p"), None, None), |_| {
            Some(root.clone())
        })
        .expect("known project");

        assert_eq!(inventory.sources.len(), 5);
        assert!(inventory.sources.iter().all(|s| s.status == ConfigSourceStatus::Available));
        assert_eq!(inventory.settings.len(), 5);
    }

    #[test]
    fn config_profile_inventory_agent_filter_narrows_the_sources_read() {
        let (home, project) = seeded();
        let root = project.path().to_string_lossy().to_string();

        let inventory = build(
            Some(home.path()),
            &request(Some("p"), Some(ConfigAgent::Codex), None),
            |_| Some(root.clone()),
        )
        .expect("known project");

        assert_eq!(inventory.sources.len(), 2);
        assert!(inventory.sources.iter().all(|s| s.agent == ConfigAgent::Codex));
    }

    #[test]
    fn config_profile_inventory_scope_filter_narrows_the_sources_read() {
        let (home, project) = seeded();
        let root = project.path().to_string_lossy().to_string();

        let inventory = build(
            Some(home.path()),
            &request(Some("p"), None, Some(ConfigScope::ProjectLocal)),
            |_| Some(root.clone()),
        )
        .expect("known project");

        assert_eq!(inventory.sources.len(), 1);
        assert_eq!(inventory.sources[0].scope, ConfigScope::ProjectLocal);
    }

    #[test]
    fn config_profile_inventory_unknown_project_is_a_stable_not_found() {
        let (home, _project) = seeded();

        let error = build(Some(home.path()), &request(Some("missing"), None, None), |_| None)
            .expect_err("unknown project is rejected");

        assert_eq!(error.kind, ErrorKind::NotFound);
        assert_eq!(error.message, "project_not_found");
    }

    #[test]
    fn config_profile_inventory_unresolvable_home_leaves_project_sources_usable() {
        let (_home, project) = seeded();
        let root = project.path().to_string_lossy().to_string();

        let inventory =
            build(None, &request(Some("p"), None, None), |_| Some(root.clone())).expect("project");

        for source in &inventory.sources {
            match source.scope {
                ConfigScope::User => {
                    assert_eq!(source.status, ConfigSourceStatus::Unreadable);
                    assert_eq!(source.fingerprint, None);
                }
                _ => assert_eq!(source.status, ConfigSourceStatus::Available),
            }
        }
    }
}
