//! Config Profile management commands.
//!
//! Every request carries opaque ids and typed allowlisted scalars. None of them
//! can carry a path, a scope, a raw document or an arbitrary key: the request
//! types deny unknown fields, and every path is composed in
//! `core::config_profile_management` from the fixed table plus the Project root
//! that the store — not the caller — supplies.
//!
//! Failures reach the frontend as one stable code. A parser message or an OS
//! error string could carry a path or a fragment of the user's configuration,
//! so neither is ever forwarded.

use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use crate::core::config_profile_management::{
    self as management, ApplyConfigProfileRequest, ConfigProfileApplyOutcome,
    ConfigProfileAssignmentDto, ConfigProfileDto, ConfigProfileEnv, ConfigProfileError,
    ConfigProfileKeyDto, ConfigProfilePreviewDto, ConfigProfileWriteEnv,
    CreateConfigProfileRequest, DeleteConfigProfileRequest, PreviewConfigProfileApplyRequest,
    PreviewConfigProfileRestoreRequest, PreviewStore, RemoveConfigProfileAssignmentRequest,
    SetConfigProfileAssignmentRequest, UpdateConfigProfileRequest,
};
use crate::core::error::{AppError, ErrorKind};
use crate::core::library_availability;
use crate::core::skill_store::SkillStore;

/// Outstanding apply and restore previews. Memory only: a token that survived a
/// restart would authorize a write against a source nobody has looked at since.
#[derive(Default)]
pub struct ConfigProfileState {
    pub previews: PreviewStore,
}

/// Maps the stable code onto the kind the frontend uses to choose a
/// presentation. The message is always the code itself.
fn error(error: ConfigProfileError) -> AppError {
    let kind = match error {
        ConfigProfileError::ProfileNotFound
        | ConfigProfileError::ProjectNotFound
        | ConfigProfileError::RecoveryNotFound => ErrorKind::NotFound,
        ConfigProfileError::InvalidProfileEntry
        | ConfigProfileError::SourceInvalid
        | ConfigProfileError::UnsupportedSymlink
        | ConfigProfileError::TooLarge
        // A stale profile, an in-use profile and a stale or expired preview are
        // all "your view of the state is out of date": the frontend re-reads and
        // asks again rather than reporting a failure.
        | ConfigProfileError::StaleProfile
        | ConfigProfileError::ProfileInUse
        | ConfigProfileError::StalePreview
        | ConfigProfileError::PreviewExpired => ErrorKind::InvalidInput,
        ConfigProfileError::LibraryOffline => ErrorKind::LibraryOffline,
        ConfigProfileError::WriteFailed
        | ConfigProfileError::AtomicReplaceUnsupported
        | ConfigProfileError::RollbackFailed => ErrorKind::Internal,
    };
    AppError {
        kind,
        message: error.as_str().to_string(),
    }
}

/// Builds the environment for one operation.
///
/// The Library state is read once per call rather than cached: a mutation that
/// starts while the Library is going offline must see the state at its own
/// start, not at application launch.
fn env(store: &Arc<SkillStore>) -> ConfigProfileEnv<'_> {
    ConfigProfileEnv {
        store: store.as_ref(),
        library_online: library_availability::availability().is_online(),
    }
}

#[tauri::command]
pub async fn list_config_profiles(
    store: State<'_, Arc<SkillStore>>,
) -> Result<Vec<ConfigProfileDto>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::list_profiles(&env(&store)).map_err(error)
    })
    .await?
}

/// The keys a profile may carry, so the editor renders typed controls without
/// knowing the allowlist itself.
#[tauri::command]
pub async fn list_config_profile_keys() -> Result<Vec<ConfigProfileKeyDto>, AppError> {
    Ok(management::writable_keys())
}

#[tauri::command]
pub async fn create_config_profile(
    store: State<'_, Arc<SkillStore>>,
    request: CreateConfigProfileRequest,
) -> Result<ConfigProfileDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::create_profile(&env(&store), &request, Utc::now().timestamp()).map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn update_config_profile(
    store: State<'_, Arc<SkillStore>>,
    request: UpdateConfigProfileRequest,
) -> Result<ConfigProfileDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::update_profile(&env(&store), &request, Utc::now().timestamp()).map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn delete_config_profile(
    store: State<'_, Arc<SkillStore>>,
    request: DeleteConfigProfileRequest,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::delete_profile(&env(&store), &request).map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn list_config_profile_assignments(
    store: State<'_, Arc<SkillStore>>,
    profile_id: Option<String>,
) -> Result<Vec<ConfigProfileAssignmentDto>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::list_assignments(&env(&store), profile_id.as_deref()).map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn set_config_profile_assignment(
    store: State<'_, Arc<SkillStore>>,
    request: SetConfigProfileAssignmentRequest,
) -> Result<ConfigProfileAssignmentDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::set_assignment(&env(&store), &request).map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn remove_config_profile_assignment(
    store: State<'_, Arc<SkillStore>>,
    request: RemoveConfigProfileAssignmentRequest,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::remove_assignment(&env(&store), &request).map_err(error)
    })
    .await?
}

/// Produces the diff the user confirms. Creates no file, no directory and no
/// database row.
#[tauri::command]
pub async fn preview_config_profile_apply(
    store: State<'_, Arc<SkillStore>>,
    state: State<'_, Arc<ConfigProfileState>>,
    request: PreviewConfigProfileApplyRequest,
) -> Result<ConfigProfilePreviewDto, AppError> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        management::preview_apply(
            &env(&store),
            &state.previews,
            &request,
            Utc::now().timestamp(),
        )
        .map_err(error)
    })
    .await?
}

/// Where owner-private recovery payloads live: application state, never the
/// Library Git tree that gets synced and backed up.
fn recovery_root() -> std::path::PathBuf {
    crate::core::central_repo::base_dir().join("config-profile-recovery")
}

/// Applies the confirmed preview. The token is the whole request: every input
/// the write depends on is re-derived and revalidated under the write lock.
#[tauri::command]
pub async fn apply_config_profile(
    store: State<'_, Arc<SkillStore>>,
    state: State<'_, Arc<ConfigProfileState>>,
    request: ApplyConfigProfileRequest,
) -> Result<ConfigProfileApplyOutcome, AppError> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = recovery_root();
        let write_env = ConfigProfileWriteEnv {
            profile: env(&store),
            recovery_root: &root,
            fault: None,
        };
        management::apply_config_profile(
            &write_env,
            &state.previews,
            &request,
            Utc::now().timestamp(),
        )
        .map_err(error)
    })
    .await?
}

/// Shows what the latest recovery point would put back. Writes nothing.
#[tauri::command]
pub async fn preview_config_profile_restore(
    store: State<'_, Arc<SkillStore>>,
    state: State<'_, Arc<ConfigProfileState>>,
    request: PreviewConfigProfileRestoreRequest,
) -> Result<ConfigProfilePreviewDto, AppError> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = recovery_root();
        let write_env = ConfigProfileWriteEnv {
            profile: env(&store),
            recovery_root: &root,
            fault: None,
        };
        management::preview_restore(
            &write_env,
            &state.previews,
            &request,
            Utc::now().timestamp(),
        )
        .map_err(error)
    })
    .await?
}

#[tauri::command]
pub async fn apply_config_profile_restore(
    store: State<'_, Arc<SkillStore>>,
    state: State<'_, Arc<ConfigProfileState>>,
    request: ApplyConfigProfileRequest,
) -> Result<ConfigProfileApplyOutcome, AppError> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = recovery_root();
        let write_env = ConfigProfileWriteEnv {
            profile: env(&store),
            recovery_root: &root,
            fault: None,
        };
        management::apply_config_profile_restore(
            &write_env,
            &state.previews,
            &request,
            Utc::now().timestamp(),
        )
        .map_err(error)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command layer forwards a code, never a sentence. A message with a
    /// space, a slash or a quote would be a parser or OS string that had
    /// escaped.
    #[test]
    fn command_errors_are_stable_codes_without_source_content() {
        for variant in [
            ConfigProfileError::ProfileNotFound,
            ConfigProfileError::ProjectNotFound,
            ConfigProfileError::InvalidProfileEntry,
            ConfigProfileError::StaleProfile,
            ConfigProfileError::ProfileInUse,
            ConfigProfileError::LibraryOffline,
            ConfigProfileError::SourceInvalid,
            ConfigProfileError::UnsupportedSymlink,
            ConfigProfileError::TooLarge,
            ConfigProfileError::StalePreview,
            ConfigProfileError::PreviewExpired,
            ConfigProfileError::WriteFailed,
            ConfigProfileError::AtomicReplaceUnsupported,
            ConfigProfileError::RollbackFailed,
            ConfigProfileError::RecoveryNotFound,
        ] {
            let mapped = error(variant);
            assert_eq!(mapped.message, variant.as_str());
            assert!(!mapped.message.contains(' '));
            assert!(!mapped.message.contains('/'));
            assert!(!mapped.message.contains('"'));
        }
    }

    /// `apply_config_profile` takes the token and nothing else, so this is the
    /// shape the command boundary has to keep accepting.
    #[test]
    fn apply_request_carries_only_a_token() {
        let parsed: ApplyConfigProfileRequest = serde_json::from_str(r#"{"token":"abc"}"#).unwrap();
        assert_eq!(parsed.token, "abc");
        assert!(serde_json::from_str::<ApplyConfigProfileRequest>(
            r#"{"token":"abc","projectId":"alpha"}"#
        )
        .is_err());
    }
}
