//! Config Profile persistence, assignment and preview-first source mutation.
//!
//! Everything here is derived by the backend from a saved profile plus a
//! registered Project record. A caller supplies opaque ids and typed
//! allowlisted scalars — never a path, a scope, a raw document or an arbitrary
//! key.
//!
//! See `openspec/changes/manage-codex-claude-config-profiles`.

use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactDeploymentRecord, ArtifactScope, DeploymentMode, HookBackupKind};
use super::config_profile_inventory::{
    allowlisted_key, allowlisted_keys, AllowlistedKey, ConfigAgent, ConfigDiffStatus, ConfigFormat,
    ConfigValueDto, ConfigValueKind, MAX_SOURCE_BYTES,
};
use super::skill_store::{
    ConfigProfileEntryRecord, ConfigProfileRecord, ConfigProfileRecoveryRecord, SkillStore,
};

/// The longest profile name that can be stored.
///
/// A name is displayed in a list and a confirmation dialog, so it is bounded
/// for the same reason the sources are: an unbounded string is a payload.
const MAX_NAME_BYTES: usize = 120;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Every failure a Config Profile operation can report.
///
/// The frontend branches on the snake_case string, so these variants are part
/// of the IPC contract. The code is the whole message: a parser string or an OS
/// error string could carry a path or a fragment of the user's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigProfileError {
    ProfileNotFound,
    ProjectNotFound,
    InvalidProfileEntry,
    StaleProfile,
    ProfileInUse,
    LibraryOffline,
    SourceInvalid,
    UnsupportedSymlink,
    TooLarge,
    StalePreview,
    PreviewExpired,
    WriteFailed,
    AtomicReplaceUnsupported,
    RollbackFailed,
    RecoveryNotFound,
}

impl ConfigProfileError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigProfileError::ProfileNotFound => "profile_not_found",
            ConfigProfileError::ProjectNotFound => "project_not_found",
            ConfigProfileError::InvalidProfileEntry => "invalid_profile_entry",
            ConfigProfileError::StaleProfile => "stale_profile",
            ConfigProfileError::ProfileInUse => "profile_in_use",
            ConfigProfileError::LibraryOffline => "library_offline",
            ConfigProfileError::SourceInvalid => "source_invalid",
            ConfigProfileError::UnsupportedSymlink => "unsupported_symlink",
            ConfigProfileError::TooLarge => "too_large",
            ConfigProfileError::StalePreview => "stale_preview",
            ConfigProfileError::PreviewExpired => "preview_expired",
            ConfigProfileError::WriteFailed => "write_failed",
            ConfigProfileError::AtomicReplaceUnsupported => "atomic_replace_unsupported",
            ConfigProfileError::RollbackFailed => "rollback_failed",
            ConfigProfileError::RecoveryNotFound => "recovery_not_found",
        }
    }
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Everything a profile operation needs beyond its request.
///
/// `library_online` is passed in rather than read from global state so the
/// offline gate is a value a test can set, not a process the test has to
/// arrange.
pub struct ConfigProfileEnv<'a> {
    pub store: &'a SkillStore,
    pub library_online: bool,
}

impl ConfigProfileEnv<'_> {
    /// Refuses before any persistent mutation when the Library is offline.
    ///
    /// Inspection stays available: this gate is on the write path only.
    fn ensure_writable(&self) -> Result<(), ConfigProfileError> {
        if self.library_online {
            Ok(())
        } else {
            Err(ConfigProfileError::LibraryOffline)
        }
    }
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// One setting a profile carries, as the frontend sends it.
///
/// `deny_unknown_fields` is the enforcement: an entry carrying `path`, `env`,
/// `raw` or a nested object fails to deserialize before any validation runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigProfileEntryDto {
    pub agent: ConfigAgent,
    pub canonical_key: String,
    pub value: ConfigValueDto,
}

/// Everything the frontend may say when creating a profile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateConfigProfileRequest {
    pub name: String,
    #[serde(default)]
    pub entries: Vec<ConfigProfileEntryDto>,
}

/// Everything the frontend may say when saving an edited profile.
///
/// `expected_revision` is the revision the editor was opened against, so two
/// editors on one profile cannot silently overwrite each other.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConfigProfileRequest {
    pub profile_id: String,
    pub expected_revision: i64,
    pub name: String,
    #[serde(default)]
    pub entries: Vec<ConfigProfileEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteConfigProfileRequest {
    pub profile_id: String,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

/// One saved profile as the frontend sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileDto {
    pub id: String,
    pub name: String,
    pub revision: i64,
    pub entries: Vec<ConfigProfileEntryDto>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One writable setting, so the editor can build a typed control without
/// knowing the allowlist itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileKeyDto {
    pub agent: ConfigAgent,
    pub canonical_key: String,
    pub value_kind: ConfigValueKind,
}

/// The complete set of keys a profile may carry, in allowlist order.
///
/// The editor renders exactly these controls, which is what keeps an arbitrary
/// key from being expressible in the UI at all.
pub fn writable_keys() -> Vec<ConfigProfileKeyDto> {
    [ConfigAgent::Codex, ConfigAgent::ClaudeCode]
        .into_iter()
        .flat_map(|agent| {
            allowlisted_keys(agent)
                .into_iter()
                .map(move |key| ConfigProfileKeyDto {
                    agent,
                    canonical_key: key.canonical.to_string(),
                    value_kind: key.kind,
                })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Checks a name against what a profile list and a confirmation dialog can
/// display.
fn validated_name(name: &str) -> Result<String, ConfigProfileError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_NAME_BYTES {
        return Err(ConfigProfileError::InvalidProfileEntry);
    }
    Ok(trimmed.to_string())
}

/// Checks every entry against the inspection allowlist and rejects the request
/// as a whole on the first failure.
///
/// Validation is re-run here even though the frontend only offers allowlisted
/// controls: the command boundary is reachable without the UI, and a stored
/// entry outside the allowlist would have no transform to apply it with.
///
/// The rejected key or value is never named in the error — a request that
/// smuggled a credential in as a key would otherwise have it echoed back.
fn validated_entries(
    entries: &[ConfigProfileEntryDto],
) -> Result<Vec<ConfigProfileEntryRecord>, ConfigProfileError> {
    let mut seen: Vec<(ConfigAgent, &str)> = Vec::with_capacity(entries.len());
    let mut records = Vec::with_capacity(entries.len());

    for entry in entries {
        let key = allowlisted_key(entry.agent, &entry.canonical_key)
            .ok_or(ConfigProfileError::InvalidProfileEntry)?;
        if key.kind != entry.value.kind() {
            return Err(ConfigProfileError::InvalidProfileEntry);
        }
        if let ConfigValueDto::String(value) = &entry.value {
            // A value is written into the user's config verbatim, so it has the
            // same bound as the keys around it.
            if value.len() > MAX_NAME_BYTES {
                return Err(ConfigProfileError::InvalidProfileEntry);
            }
        }
        let identity = (entry.agent, key.canonical);
        if seen.contains(&identity) {
            return Err(ConfigProfileError::InvalidProfileEntry);
        }
        seen.push(identity);
        records.push(ConfigProfileEntryRecord {
            agent: entry.agent,
            canonical_key: key.canonical.to_string(),
            value: entry.value.clone(),
        });
    }
    Ok(records)
}

fn to_dto(record: ConfigProfileRecord) -> ConfigProfileDto {
    ConfigProfileDto {
        id: record.artifact_id,
        name: record.name,
        revision: record.revision,
        entries: record
            .entries
            .into_iter()
            .map(|entry| ConfigProfileEntryDto {
                agent: entry.agent,
                canonical_key: entry.canonical_key,
                value: entry.value,
            })
            .collect(),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/// Every saved profile. Reading does not require an online Library: the profile
/// list is inspection of AgentDeck's own state.
pub fn list_profiles(
    env: &ConfigProfileEnv<'_>,
) -> Result<Vec<ConfigProfileDto>, ConfigProfileError> {
    let records = env
        .store
        .list_config_profiles()
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(records.into_iter().map(to_dto).collect())
}

pub fn create_profile(
    env: &ConfigProfileEnv<'_>,
    request: &CreateConfigProfileRequest,
    now: i64,
) -> Result<ConfigProfileDto, ConfigProfileError> {
    env.ensure_writable()?;
    // Validated before the transaction opens, so a rejected request never
    // reaches SQLite at all.
    let name = validated_name(&request.name)?;
    let entries = validated_entries(&request.entries)?;

    let record = ConfigProfileRecord {
        artifact_id: uuid::Uuid::new_v4().to_string(),
        name,
        revision: 1,
        created_at: now,
        updated_at: now,
        entries,
    };
    env.store
        .create_config_profile(&record)
        .map_err(|_| ConfigProfileError::InvalidProfileEntry)?;
    Ok(to_dto(record))
}

/// Saves a name and a complete entry set against the revision the editor was
/// opened on.
///
/// The revision advances only when the saved state actually differs. A save
/// that changes nothing would otherwise invalidate every outstanding apply
/// preview for no reason.
pub fn update_profile(
    env: &ConfigProfileEnv<'_>,
    request: &UpdateConfigProfileRequest,
    now: i64,
) -> Result<ConfigProfileDto, ConfigProfileError> {
    env.ensure_writable()?;
    let name = validated_name(&request.name)?;
    let entries = validated_entries(&request.entries)?;

    let current = env
        .store
        .get_config_profile(&request.profile_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .ok_or(ConfigProfileError::ProfileNotFound)?;
    if current.revision != request.expected_revision {
        return Err(ConfigProfileError::StaleProfile);
    }

    let unchanged = current.name == name && same_entry_set(&current.entries, &entries);
    if unchanged {
        return Ok(to_dto(current));
    }

    let record = ConfigProfileRecord {
        artifact_id: current.artifact_id.clone(),
        name,
        revision: current.revision + 1,
        created_at: current.created_at,
        updated_at: now,
        entries,
    };
    let applied = env
        .store
        .update_config_profile(&record, request.expected_revision)
        .map_err(|_| ConfigProfileError::InvalidProfileEntry)?;
    if !applied {
        // The revision moved between the read and the write.
        return Err(ConfigProfileError::StaleProfile);
    }
    Ok(to_dto(record))
}

/// Order is not part of a profile's identity — the store returns entries sorted
/// and the editor may send them in any order — so the comparison is set-wise.
fn same_entry_set(left: &[ConfigProfileEntryRecord], right: &[ConfigProfileEntryRecord]) -> bool {
    left.len() == right.len() && right.iter().all(|entry| left.contains(entry))
}

/// Deletes a profile that nothing depends on.
///
/// An assigned profile keeps its rows: the assignment names a Project whose
/// config it may already have written, and the recovery point is the only way
/// back from that write.
pub fn delete_profile(
    env: &ConfigProfileEnv<'_>,
    request: &DeleteConfigProfileRequest,
) -> Result<(), ConfigProfileError> {
    env.ensure_writable()?;
    let profile = env
        .store
        .get_config_profile(&request.profile_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .ok_or(ConfigProfileError::ProfileNotFound)?;

    let (deployments, recoveries) = env
        .store
        .count_config_profile_dependents(&profile.artifact_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    if deployments > 0 || recoveries > 0 {
        return Err(ConfigProfileError::ProfileInUse);
    }

    env.store
        .delete_config_profile(&profile.artifact_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Assignments
// ---------------------------------------------------------------------------

/// The fixed source one Agent writes inside a Project.
///
/// This is a total function of the Agent alone: there is no writable user
/// source and no writable project-local source, so neither is expressible.
pub fn project_source_id(agent: ConfigAgent) -> &'static str {
    match agent {
        ConfigAgent::Codex => "codex:project:config-toml",
        ConfigAgent::ClaudeCode => "claude_code:project:settings-json",
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetConfigProfileAssignmentRequest {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveConfigProfileAssignmentRequest {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
}

/// One profile assigned to one Project and Agent.
///
/// There is no path here: the frontend addresses an assignment by its tuple and
/// displays it by its fixed source id.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileAssignmentDto {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
    pub source_id: String,
    pub status: String,
    pub last_applied_fingerprint: Option<String>,
    pub last_applied_at: Option<i64>,
    pub has_recovery_point: bool,
}

/// The status a fresh assignment carries: it names a target nothing has been
/// written to yet.
const ASSIGNMENT_STATUS_PENDING: &str = "pending";

/// Resolves a Project id to its stored root, or reports that it is not
/// registered.
///
/// An unresolved id is an error rather than a fallback: writing to the process
/// working directory would mutate a Project the user never linked.
fn registered_project_root(
    env: &ConfigProfileEnv<'_>,
    project_id: &str,
) -> Result<std::path::PathBuf, ConfigProfileError> {
    env.store
        .get_project_by_id(project_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .map(|record| std::path::PathBuf::from(record.path))
        .ok_or(ConfigProfileError::ProjectNotFound)
}

fn loaded_profile(
    env: &ConfigProfileEnv<'_>,
    profile_id: &str,
) -> Result<ConfigProfileRecord, ConfigProfileError> {
    env.store
        .get_config_profile(profile_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .ok_or(ConfigProfileError::ProfileNotFound)
}

/// Records that a profile applies to one Project and Agent.
///
/// This writes metadata only. The Project's configuration is untouched until
/// the user previews and confirms an apply.
pub fn set_assignment(
    env: &ConfigProfileEnv<'_>,
    request: &SetConfigProfileAssignmentRequest,
) -> Result<ConfigProfileAssignmentDto, ConfigProfileError> {
    env.ensure_writable()?;
    let profile = loaded_profile(env, &request.profile_id)?;
    // Resolved for its existence, not for its path: an assignment stores no
    // root, so a moved Project cannot leave a stale target behind.
    registered_project_root(env, &request.project_id)?;

    let existing = find_deployment(
        env,
        &profile.artifact_id,
        &request.project_id,
        request.agent,
    )?;
    let deployment = ArtifactDeploymentRecord {
        id: existing
            .as_ref()
            .map(|row| row.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        artifact_id: profile.artifact_id.clone(),
        scope: ArtifactScope::Project(request.project_id.clone()),
        agent: request.agent.as_str().to_string(),
        enabled: true,
        mode: DeploymentMode::ConfigProfile,
        // The target is re-derived from the Project record and the Agent on
        // every use, so neither path is persisted.
        source_path: String::new(),
        target_path: String::new(),
        last_synced_hash: existing
            .as_ref()
            .and_then(|row| row.last_synced_hash.clone()),
        last_synced_at: existing.as_ref().and_then(|row| row.last_synced_at),
        status: existing
            .as_ref()
            .map(|row| row.status.clone())
            .unwrap_or_else(|| ASSIGNMENT_STATUS_PENDING.to_string()),
        last_error: None,
    };
    env.store
        .upsert_deployment(&deployment)
        .map_err(|_| ConfigProfileError::WriteFailed)?;

    to_assignment_dto(env, &deployment)
}

/// Removes one assignment identity.
///
/// A recovery point blocks removal rather than being discarded with it: it is
/// the only way back from a write this assignment made, and dropping it would
/// strand the Project's configuration in the applied state with no undo.
pub fn remove_assignment(
    env: &ConfigProfileEnv<'_>,
    request: &RemoveConfigProfileAssignmentRequest,
) -> Result<(), ConfigProfileError> {
    env.ensure_writable()?;
    let profile = loaded_profile(env, &request.profile_id)?;

    let recovery = env
        .store
        .get_config_profile_recovery(&profile.artifact_id, &request.project_id, request.agent)
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    if recovery.is_some() {
        return Err(ConfigProfileError::ProfileInUse);
    }

    env.store
        .delete_deployment(
            &profile.artifact_id,
            &ArtifactScope::Project(request.project_id.clone()),
            request.agent.as_str(),
        )
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(())
}

/// Every assignment, or every assignment of one profile.
pub fn list_assignments(
    env: &ConfigProfileEnv<'_>,
    profile_id: Option<&str>,
) -> Result<Vec<ConfigProfileAssignmentDto>, ConfigProfileError> {
    let deployments = match profile_id {
        Some(id) => env
            .store
            .get_deployments_for_artifact(id)
            .map_err(|_| ConfigProfileError::WriteFailed)?,
        None => env
            .store
            .get_all_deployments()
            .map_err(|_| ConfigProfileError::WriteFailed)?,
    };
    deployments
        .iter()
        .filter(|row| row.mode == DeploymentMode::ConfigProfile)
        .map(|row| to_assignment_dto(env, row))
        .collect()
}

fn find_deployment(
    env: &ConfigProfileEnv<'_>,
    artifact_id: &str,
    project_id: &str,
    agent: ConfigAgent,
) -> Result<Option<ArtifactDeploymentRecord>, ConfigProfileError> {
    let rows = env
        .store
        .get_deployments_for_artifact(artifact_id)
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(rows.into_iter().find(|row| {
        row.scope == ArtifactScope::Project(project_id.to_string()) && row.agent == agent.as_str()
    }))
}

fn to_assignment_dto(
    env: &ConfigProfileEnv<'_>,
    deployment: &ArtifactDeploymentRecord,
) -> Result<ConfigProfileAssignmentDto, ConfigProfileError> {
    let agent =
        ConfigAgent::parse(&deployment.agent).ok_or(ConfigProfileError::InvalidProfileEntry)?;
    let project_id = deployment.scope.scope_id().to_string();
    let has_recovery_point = env
        .store
        .get_config_profile_recovery(&deployment.artifact_id, &project_id, agent)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .is_some();

    Ok(ConfigProfileAssignmentDto {
        profile_id: deployment.artifact_id.clone(),
        project_id,
        agent,
        source_id: project_source_id(agent).to_string(),
        status: deployment.status.clone(),
        last_applied_fingerprint: deployment.last_synced_hash.clone(),
        last_applied_at: deployment.last_synced_at,
        has_recovery_point,
    })
}

// ---------------------------------------------------------------------------
// Fixed project targets
// ---------------------------------------------------------------------------

/// Whether the resolved target already exists.
///
/// Both states are writable; every other file type is refused before any
/// mutation starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTargetState {
    Missing,
    Present,
}

/// A resolved, writable Config Profile source.
///
/// Only this module produces one, and only from a registered Project record
/// plus an Agent — never from caller-supplied text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigProfileTarget {
    pub source_id: &'static str,
    pub agent: ConfigAgent,
    pub format: ConfigFormat,
    pub path: std::path::PathBuf,
    pub state: ConfigTargetState,
}

/// The parsed form of a target document, ready to be transformed.
#[derive(Debug)]
pub enum ConfigDocument {
    Toml(Box<toml_edit::DocumentMut>),
    Json(Box<serde_json::Map<String, serde_json::Value>>),
}

/// Resolves one registered Project and Agent to its fixed writable source.
///
/// Resolution is read-only: it creates no directory, no file and no database
/// row, so a refused request leaves the disk untouched.
pub fn resolve_target(
    env: &ConfigProfileEnv<'_>,
    project_id: &str,
    agent: ConfigAgent,
) -> Result<ConfigProfileTarget, ConfigProfileError> {
    let root = registered_project_root(env, project_id)?;
    if !root.is_dir() {
        // A Project record whose root has gone. Creating it would put a config
        // where the user no longer has a Project.
        return Err(ConfigProfileError::SourceInvalid);
    }

    let (format, path) = match agent {
        ConfigAgent::Codex => (ConfigFormat::Toml, root.join(".codex").join("config.toml")),
        ConfigAgent::ClaudeCode => (
            ConfigFormat::Json,
            root.join(".claude").join("settings.json"),
        ),
    };

    // `symlink_metadata` does not follow the link, so a symlinked source is
    // refused instead of being replaced by its target — which would write
    // outside the approved Project boundary.
    let state = match std::fs::symlink_metadata(&path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ConfigTargetState::Missing,
        Err(_) => return Err(ConfigProfileError::SourceInvalid),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ConfigProfileError::UnsupportedSymlink)
        }
        Ok(metadata) if metadata.file_type().is_file() => ConfigTargetState::Present,
        Ok(_) => return Err(ConfigProfileError::SourceInvalid),
    };

    Ok(ConfigProfileTarget {
        source_id: project_source_id(agent),
        agent,
        format,
        path,
        state,
    })
}

/// Reads the whole source, or reports that it is not there.
///
/// The same read limit as inspection applies: a config past it is refused
/// rather than pulled into memory.
pub fn read_target(target: &ConfigProfileTarget) -> Result<Option<String>, ConfigProfileError> {
    if target.state == ConfigTargetState::Missing {
        return Ok(None);
    }
    let metadata =
        std::fs::metadata(&target.path).map_err(|_| ConfigProfileError::SourceInvalid)?;
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(ConfigProfileError::TooLarge);
    }
    match std::fs::read_to_string(&target.path) {
        Ok(text) => Ok(Some(text)),
        // Not valid UTF-8 is not a config document this capability can edit.
        Err(_) => Err(ConfigProfileError::SourceInvalid),
    }
}

/// Parses the source, or produces the minimal empty document for a target that
/// is not there yet.
///
/// An invalid document is never repaired or replaced: rewriting it would
/// silently discard whatever the user actually has.
pub fn parse_target(
    target: &ConfigProfileTarget,
    text: Option<&str>,
) -> Result<ConfigDocument, ConfigProfileError> {
    match (target.format, text) {
        (ConfigFormat::Toml, None) => Ok(ConfigDocument::Toml(Box::default())),
        (ConfigFormat::Json, None) => Ok(ConfigDocument::Json(Box::default())),
        (ConfigFormat::Toml, Some(text)) => text
            .parse::<toml_edit::DocumentMut>()
            .map(|document| ConfigDocument::Toml(Box::new(document)))
            .map_err(|_| ConfigProfileError::SourceInvalid),
        (ConfigFormat::Json, Some(text)) => match serde_json::from_str(text) {
            Ok(serde_json::Value::Object(map)) => Ok(ConfigDocument::Json(Box::new(map))),
            // A JSON document that is not an object has no top-level keys to
            // set, so it is as unusable as a parse failure.
            _ => Err(ConfigProfileError::SourceInvalid),
        },
    }
}

// ---------------------------------------------------------------------------
// Agent-specific transformation
// ---------------------------------------------------------------------------

/// One allowlisted setting as it changes between the current source and the
/// document an apply would write.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileDiffEntryDto {
    pub agent: ConfigAgent,
    pub canonical_key: String,
    pub status: ConfigDiffStatus,
    pub before: Option<ConfigValueDto>,
    pub after: Option<ConfigValueDto>,
}

/// The document an apply would write, plus the typed diff that justifies it.
#[derive(Debug)]
pub struct ConfigTransform {
    pub document_text: String,
    pub diff: Vec<ConfigProfileDiffEntryDto>,
}

/// Reads one allowlisted key out of a parsed document.
///
/// A key present with a shape the allowlist does not accept reads as absent
/// here: the transform overwrites it with the profile's typed value, which is
/// the only shape this capability can express.
fn current_value(document: &ConfigDocument, key: &AllowlistedKey) -> Option<ConfigValueDto> {
    match document {
        ConfigDocument::Toml(document) => {
            let item = document.get(key.native)?;
            match key.kind {
                ConfigValueKind::String => {
                    item.as_str().map(|v| ConfigValueDto::String(v.to_string()))
                }
                ConfigValueKind::Boolean => item.as_bool().map(ConfigValueDto::Boolean),
                ConfigValueKind::Integer => item.as_integer().map(ConfigValueDto::Integer),
            }
        }
        ConfigDocument::Json(map) => {
            let value = json_leaf(map, key.native)?;
            match key.kind {
                ConfigValueKind::String => value
                    .as_str()
                    .map(|v| ConfigValueDto::String(v.to_string())),
                ConfigValueKind::Boolean => value.as_bool().map(ConfigValueDto::Boolean),
                ConfigValueKind::Integer => value.as_i64().map(ConfigValueDto::Integer),
            }
        }
    }
}

/// Resolves a native key that may name one nested leaf, such as
/// `permissions.defaultMode`.
///
/// The path is fixed by the allowlist, never composed from a request, so there
/// is no arbitrary traversal here.
fn json_leaf<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    native: &str,
) -> Option<&'a serde_json::Value> {
    match native.split_once('.') {
        None => map.get(native),
        Some((parent, leaf)) => map.get(parent)?.as_object()?.get(leaf),
    }
}

/// Applies one profile's entries for one Agent to the target document.
///
/// Only the exact allowlisted keys the profile carries are touched. Unknown
/// keys, unknown tables, comments, ordering and nested siblings are the
/// document's own and are left alone — which is why the TOML side edits a
/// `DocumentMut` instead of serializing a fresh document.
pub fn transform_target(
    target: &ConfigProfileTarget,
    source_text: Option<&str>,
    entries: &[ConfigProfileEntryRecord],
) -> Result<ConfigTransform, ConfigProfileError> {
    let mut document = parse_target(target, source_text)?;
    let mut diff = Vec::new();

    for entry in entries.iter().filter(|entry| entry.agent == target.agent) {
        let key = allowlisted_key(entry.agent, &entry.canonical_key)
            .ok_or(ConfigProfileError::InvalidProfileEntry)?;
        let before = current_value(&document, &key);
        let status = match &before {
            None => ConfigDiffStatus::Added,
            Some(current) if *current == entry.value => ConfigDiffStatus::Same,
            Some(_) => ConfigDiffStatus::Changed,
        };
        if status != ConfigDiffStatus::Same {
            set_value(&mut document, &key, &entry.value);
        }
        diff.push(ConfigProfileDiffEntryDto {
            agent: entry.agent,
            canonical_key: key.canonical.to_string(),
            status,
            before,
            after: Some(entry.value.clone()),
        });
    }

    diff.sort_by(|a, b| a.canonical_key.cmp(&b.canonical_key));
    let document_text = render(&document);

    // Fails closed: a document that no longer parses, or that does not read
    // back every value just written, never becomes a staged file.
    verify_round_trip(target, &document_text, entries)?;

    Ok(ConfigTransform {
        document_text,
        diff,
    })
}

/// Writes one typed value at the key's exact location.
fn set_value(document: &mut ConfigDocument, key: &AllowlistedKey, value: &ConfigValueDto) {
    match document {
        ConfigDocument::Toml(document) => {
            let mut replacement: toml_edit::Value = match value {
                ConfigValueDto::String(v) => v.as_str().into(),
                ConfigValueDto::Boolean(v) => (*v).into(),
                ConfigValueDto::Integer(v) => (*v).into(),
            };
            match document.get(key.native).and_then(|item| item.as_value()) {
                // The decor is the spacing around the value and anything after
                // it on the line — including a trailing comment the user wrote.
                // Only the value itself is ours to change.
                Some(existing) => {
                    *replacement.decor_mut() = existing.decor().clone();
                    document[key.native] = toml_edit::Item::Value(replacement);
                }
                // Absent, or present as a table this capability cannot express.
                // Removing it first drops the old entry's formatting, so the
                // replacement is written as a clean scalar rather than
                // inheriting a table header's spacing.
                None => {
                    document.remove(key.native);
                    document[key.native] = toml_edit::Item::Value(replacement);
                }
            }
        }
        ConfigDocument::Json(map) => {
            let leaf = match value {
                ConfigValueDto::String(v) => serde_json::Value::String(v.clone()),
                ConfigValueDto::Boolean(v) => serde_json::Value::Bool(*v),
                ConfigValueDto::Integer(v) => serde_json::Value::Number((*v).into()),
            };
            match key.native.split_once('.') {
                None => {
                    map.insert(key.native.to_string(), leaf);
                }
                Some((parent, name)) => {
                    // A parent that exists but is not an object would lose its
                    // value here, so it is replaced only when it cannot hold
                    // the leaf at all.
                    let entry = map
                        .entry(parent.to_string())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if !entry.is_object() {
                        *entry = serde_json::Value::Object(serde_json::Map::new());
                    }
                    entry
                        .as_object_mut()
                        .expect("parent was just ensured to be an object")
                        .insert(name.to_string(), leaf);
                }
            }
        }
    }
}

fn render(document: &ConfigDocument) -> String {
    match document {
        ConfigDocument::Toml(document) => document.to_string(),
        // Two-space indentation with a trailing newline is what both Agents
        // write themselves, so a round trip does not reflow the whole file.
        ConfigDocument::Json(map) => {
            let mut text = serde_json::to_string_pretty(map).unwrap_or_default();
            text.push('\n');
            text
        }
    }
}

/// Re-parses the rendered document and checks every selected entry reads back
/// exactly as written.
fn verify_round_trip(
    target: &ConfigProfileTarget,
    document_text: &str,
    entries: &[ConfigProfileEntryRecord],
) -> Result<(), ConfigProfileError> {
    let parsed =
        parse_target(target, Some(document_text)).map_err(|_| ConfigProfileError::WriteFailed)?;
    for entry in entries.iter().filter(|entry| entry.agent == target.agent) {
        let key = allowlisted_key(entry.agent, &entry.canonical_key)
            .ok_or(ConfigProfileError::InvalidProfileEntry)?;
        if current_value(&parsed, &key).as_ref() != Some(&entry.value) {
            return Err(ConfigProfileError::WriteFailed);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Preview authority
// ---------------------------------------------------------------------------

/// How long a preview token stays usable.
///
/// Long enough to read a diff and decide, short enough that a token left in a
/// forgotten dialog cannot be confirmed against a source that has since moved
/// on.
const PREVIEW_TTL_SECONDS: i64 = 300;

/// The fingerprint reported for a fixed source that does not exist yet.
///
/// A literal keeps the absent state out of the hash space, so no file content
/// can ever collide with "there was no file".
pub const ABSENT_FINGERPRINT: &str = "absent";

/// Hashes the exact bytes a mutation was based on.
///
/// The hash covers the whole document, not just the allowlisted keys: an
/// external edit to an unknown key changes the file we are about to replace, so
/// it must invalidate the preview too.
pub fn fingerprint(bytes: Option<&[u8]>) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = bytes?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Some(hex::encode(hasher.finalize()))
}

/// What kind of mutation a token authorizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigPreviewOperation {
    Apply,
    Restore,
}

/// Everything one confirmed preview binds.
///
/// Apply accepts the token alone and revalidates every field here, so a request
/// cannot widen what the user actually reviewed.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundPreview {
    pub operation: ConfigPreviewOperation,
    pub profile_id: String,
    pub profile_revision: i64,
    pub project_id: String,
    pub agent: ConfigAgent,
    pub source_id: &'static str,
    /// `None` when the preview was taken against a target that is not there.
    pub base_fingerprint: Option<String>,
    /// Hash of the exact bytes the apply must produce.
    pub output_hash: String,
    pub expires_at: i64,
}

/// The in-memory set of outstanding previews.
///
/// Previews are deliberately not persisted: a token that survived a restart
/// would authorize a write against a source nobody has looked at since.
#[derive(Default)]
pub struct PreviewStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, BoundPreview>>,
}

impl PreviewStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn issue(&self, preview: BoundPreview) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.insert(token.clone(), preview);
        token
    }

    /// Takes the token out of the store, whatever the outcome.
    ///
    /// Removing before validating is what makes a token single-use: a replay
    /// after a failed apply finds nothing, rather than getting a second attempt
    /// at the same write.
    pub fn consume(&self, token: &str, now: i64) -> Result<BoundPreview, ConfigProfileError> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let preview = entries
            .remove(token)
            .ok_or(ConfigProfileError::StalePreview)?;
        if now > preview.expires_at {
            return Err(ConfigProfileError::PreviewExpired);
        }
        Ok(preview)
    }

    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewConfigProfileApplyRequest {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
}

/// What the user is asked to confirm before a write.
///
/// Only the token and the typed diff cross this boundary: no path, no raw
/// document, no unknown key and no backup bytes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfilePreviewDto {
    pub token: String,
    pub operation: ConfigPreviewOperation,
    pub profile_id: String,
    pub profile_name: String,
    pub profile_revision: i64,
    pub project_id: String,
    pub agent: ConfigAgent,
    pub source_id: String,
    /// `None` means the target does not exist yet.
    pub base_fingerprint: Option<String>,
    pub would_create_file: bool,
    /// A restore whose recovery point records absence removes the file the
    /// matching apply created.
    pub would_remove_file: bool,
    pub diff: Vec<ConfigProfileDiffEntryDto>,
    pub expires_at: i64,
}

/// Builds the preview for one assignment.
///
/// This function has no recovery root and writes nothing by construction, so a
/// preview cannot persist anything even if a later change tries to.
pub fn preview_apply(
    env: &ConfigProfileEnv<'_>,
    previews: &PreviewStore,
    request: &PreviewConfigProfileApplyRequest,
    now: i64,
) -> Result<ConfigProfilePreviewDto, ConfigProfileError> {
    let profile = loaded_profile(env, &request.profile_id)?;
    // An apply is only meaningful for an assignment the user made: without one
    // there is no deployment identity to record the result against.
    find_deployment(
        env,
        &profile.artifact_id,
        &request.project_id,
        request.agent,
    )?
    .ok_or(ConfigProfileError::ProfileNotFound)?;

    let target = resolve_target(env, &request.project_id, request.agent)?;
    let source_text = read_target(&target)?;
    let base_fingerprint = fingerprint(source_text.as_deref().map(str::as_bytes));
    let transform = transform_target(&target, source_text.as_deref(), &profile.entries)?;

    let expires_at = now + PREVIEW_TTL_SECONDS;
    let token = previews.issue(BoundPreview {
        operation: ConfigPreviewOperation::Apply,
        profile_id: profile.artifact_id.clone(),
        profile_revision: profile.revision,
        project_id: request.project_id.clone(),
        agent: request.agent,
        source_id: target.source_id,
        base_fingerprint: base_fingerprint.clone(),
        output_hash: fingerprint(Some(transform.document_text.as_bytes())).unwrap_or_default(),
        expires_at,
    });

    Ok(ConfigProfilePreviewDto {
        token,
        operation: ConfigPreviewOperation::Apply,
        profile_id: profile.artifact_id,
        profile_name: profile.name,
        profile_revision: profile.revision,
        project_id: request.project_id.clone(),
        agent: request.agent,
        source_id: target.source_id.to_string(),
        base_fingerprint,
        would_create_file: target.state == ConfigTargetState::Missing,
        would_remove_file: false,
        diff: transform.diff,
        expires_at,
    })
}

/// An apply request carries nothing but the token.
///
/// Every input the write depends on is re-derived from the token's bindings, so
/// a caller cannot widen what the user reviewed.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyConfigProfileRequest {
    pub token: String,
}

/// The exact write a revalidated token authorizes.
#[derive(Debug)]
pub struct AuthorizedWrite {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
    pub target: ConfigProfileTarget,
    /// The source as it is right now, which becomes the recovery point.
    pub original: Option<String>,
    pub document_text: String,
    pub output_fingerprint: String,
}

/// Consumes a token and re-derives the write from current state.
///
/// The preview's own text is never trusted: the profile, the Project and the
/// source are read again and transformed again, and every binding must still
/// match. Anything that moved in between is `stale_preview` — which is a
/// refusal, not a retry, because the user reviewed a diff that no longer
/// describes what would happen.
pub fn authorize_apply(
    env: &ConfigProfileEnv<'_>,
    previews: &PreviewStore,
    request: &ApplyConfigProfileRequest,
    now: i64,
) -> Result<AuthorizedWrite, ConfigProfileError> {
    env.ensure_writable()?;
    let bound = previews.consume(&request.token, now)?;
    if bound.operation != ConfigPreviewOperation::Apply {
        return Err(ConfigProfileError::StalePreview);
    }

    let profile = loaded_profile(env, &bound.profile_id)?;
    if profile.revision != bound.profile_revision {
        return Err(ConfigProfileError::StalePreview);
    }
    find_deployment(env, &profile.artifact_id, &bound.project_id, bound.agent)?
        .ok_or(ConfigProfileError::StalePreview)?;

    let target = resolve_target(env, &bound.project_id, bound.agent)?;
    if target.source_id != bound.source_id {
        return Err(ConfigProfileError::StalePreview);
    }
    let original = read_target(&target)?;
    if fingerprint(original.as_deref().map(str::as_bytes)) != bound.base_fingerprint {
        return Err(ConfigProfileError::StalePreview);
    }

    let transform = transform_target(&target, original.as_deref(), &profile.entries)?;
    let output_fingerprint =
        fingerprint(Some(transform.document_text.as_bytes())).unwrap_or_default();
    if output_fingerprint != bound.output_hash {
        // The inputs all matched but the result did not, which means something
        // outside the bound set changed the outcome. Refuse rather than write a
        // document the user never saw.
        return Err(ConfigProfileError::StalePreview);
    }

    Ok(AuthorizedWrite {
        profile_id: profile.artifact_id,
        project_id: bound.project_id,
        agent: bound.agent,
        target,
        original,
        document_text: transform.document_text,
        output_fingerprint,
    })
}

// ---------------------------------------------------------------------------
// Atomic apply
// ---------------------------------------------------------------------------

/// Serializes every Config Profile source mutation in this process.
///
/// Two applies to the same target would otherwise interleave their recovery
/// and replacement steps, and the loser would restore over the winner.
static CONFIG_PROFILE_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The status a deployment carries once its source matches the profile.
const ASSIGNMENT_STATUS_CLEAN: &str = "clean";
/// The status a deployment carries after a failed apply: the assignment stands,
/// the source does not reflect it.
const ASSIGNMENT_STATUS_FAILED: &str = "failed";

/// Where a test makes one step fail. Production callers pass `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigProfileFaultPoint {
    RecoveryPromote,
    StagedTargetSync,
    AtomicReplace,
    PostWriteVerify,
    SqliteCommit,
    /// The replacement succeeded and putting the original back then failed.
    RollbackFailure,
    /// A platform that cannot promise atomic replacement at all.
    AtomicReplaceUnsupported,
}

/// Everything a source mutation needs beyond its request.
///
/// `recovery_root` is application state — outside the Library Git tree —
/// because a recovery payload holds the user's raw configuration.
pub struct ConfigProfileWriteEnv<'a> {
    pub profile: ConfigProfileEnv<'a>,
    pub recovery_root: &'a std::path::Path,
    /// Test-only seam; production callers pass `None`.
    pub fault: Option<ConfigProfileFaultPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileApplyOutcome {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
    /// Fingerprint of the source after the write.
    pub fingerprint: String,
    pub created_file: bool,
}

fn injected(env: &ConfigProfileWriteEnv<'_>, point: ConfigProfileFaultPoint) -> bool {
    env.fault == Some(point)
}

/// Refuses before anything is touched on a runtime that cannot promise an
/// atomic replacement.
///
/// Falling back to delete-then-rename would leave a window where the Agent
/// reads a missing or half-written config.
fn ensure_atomic_replacement(env: &ConfigProfileWriteEnv<'_>) -> Result<(), ConfigProfileError> {
    if injected(env, ConfigProfileFaultPoint::AtomicReplaceUnsupported) || !cfg!(unix) {
        return Err(ConfigProfileError::AtomicReplaceUnsupported);
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &std::path::Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn mode_of(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode_of(_path: &std::path::Path) -> Option<u32> {
    None
}

/// Writes a file and forces it to disk before anyone can depend on it.
fn write_synced(path: &std::path::Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    set_mode(path, mode)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// A rename is only durable once the directory entry itself is on disk.
fn sync_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

/// Recovery payloads hold the user's raw configuration, so the directory is
/// owner-only from the moment it exists.
fn create_private_dir(path: &std::path::Path) -> Result<(), ConfigProfileError> {
    std::fs::create_dir_all(path).map_err(|_| ConfigProfileError::WriteFailed)?;
    set_mode(path, 0o700).map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(())
}

fn staged_path(target: &std::path::Path) -> std::path::PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string());
    // Same directory, so the rename stays on one filesystem and is atomic.
    target.with_file_name(format!(".{name}.agentdeck-{}", uuid::Uuid::new_v4()))
}

/// The recovery payload's location relative to the private recovery root.
fn recovery_locator(profile_id: &str, project_id: &str, agent: ConfigAgent) -> String {
    format!("{profile_id}/{project_id}/{}/previous", agent.as_str())
}

/// Puts the target back the way the recovery point says it was.
fn restore_bytes(
    original: Option<&str>,
    path: &std::path::Path,
    mode: u32,
) -> Result<(), ConfigProfileError> {
    match original {
        Some(text) => {
            let staged = staged_path(path);
            write_synced(&staged, text.as_bytes(), mode)
                .map_err(|_| ConfigProfileError::RollbackFailed)?;
            std::fs::rename(&staged, path).map_err(|_| ConfigProfileError::RollbackFailed)?;
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => {}
            // Already absent is the state we were asked to produce.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ConfigProfileError::RollbackFailed),
        },
    }
    if let Some(parent) = path.parent() {
        let _ = sync_dir(parent);
    }
    Ok(())
}

/// Re-validates the token, backs the source up and atomically replaces it.
///
/// The order is the one that leaves a recoverable state at every step: the
/// recovery payload is durable before the target changes, and the deployment is
/// only marked applied after the new bytes have been verified on disk.
pub fn apply_config_profile(
    env: &ConfigProfileWriteEnv<'_>,
    previews: &PreviewStore,
    request: &ApplyConfigProfileRequest,
    now: i64,
) -> Result<ConfigProfileApplyOutcome, ConfigProfileError> {
    // One mutation at a time, process-wide.
    let _guard = CONFIG_PROFILE_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Both gates run before the token is consumed, so a refusal here leaves the
    // preview usable once the condition clears.
    env.profile.ensure_writable()?;
    ensure_atomic_replacement(env)?;

    let write = authorize_apply(&env.profile, previews, request, now)?;
    let created_file = write.original.is_none();

    match perform_write(env, &write, now) {
        Ok(()) => Ok(ConfigProfileApplyOutcome {
            profile_id: write.profile_id,
            project_id: write.project_id,
            agent: write.agent,
            fingerprint: write.output_fingerprint,
            created_file,
        }),
        Err(error) => {
            // The assignment stands but its source does not reflect the profile.
            mark_deployment_failed(&env.profile, &write);
            Err(error)
        }
    }
}

/// The recovery point, staged write, replacement, verification and metadata
/// commit, in the one order that leaves a recoverable state at every step.
fn perform_write(
    env: &ConfigProfileWriteEnv<'_>,
    write: &AuthorizedWrite,
    now: i64,
) -> Result<(), ConfigProfileError> {
    let locator = recovery_locator(&write.profile_id, &write.project_id, write.agent);
    let payload = env.recovery_root.join(&locator);
    let payload_dir = payload.parent().ok_or(ConfigProfileError::WriteFailed)?;

    // The recovery payload lands first: after this point the previous state is
    // reconstructable even if everything below fails.
    if let Some(original) = write.original.as_deref() {
        create_private_dir(payload_dir)?;
        let staged_payload = payload.with_extension("staged");
        write_synced(&staged_payload, original.as_bytes(), 0o600)
            .map_err(|_| ConfigProfileError::WriteFailed)?;
        if injected(env, ConfigProfileFaultPoint::RecoveryPromote) {
            let _ = std::fs::remove_file(&staged_payload);
            return Err(ConfigProfileError::WriteFailed);
        }
        std::fs::rename(&staged_payload, &payload).map_err(|_| ConfigProfileError::WriteFailed)?;
        let _ = sync_dir(payload_dir);
    }

    let parent = write
        .target
        .path
        .parent()
        .ok_or(ConfigProfileError::WriteFailed)?
        .to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|_| ConfigProfileError::WriteFailed)?;
    // A file AgentDeck creates may hold a secret, so it starts owner-only; an
    // existing file keeps whatever mode the user chose.
    let mode = mode_of(&write.target.path).unwrap_or(0o600);
    let staged_target = staged_path(&write.target.path);

    let replaced = (|| -> Result<(), ConfigProfileError> {
        write_synced(&staged_target, write.document_text.as_bytes(), mode)
            .map_err(|_| ConfigProfileError::WriteFailed)?;
        if injected(env, ConfigProfileFaultPoint::StagedTargetSync) {
            return Err(ConfigProfileError::WriteFailed);
        }
        if injected(env, ConfigProfileFaultPoint::AtomicReplace) {
            return Err(ConfigProfileError::WriteFailed);
        }
        std::fs::rename(&staged_target, &write.target.path)
            .map_err(|_| ConfigProfileError::WriteFailed)?;
        let _ = sync_dir(&parent);
        Ok(())
    })();
    if let Err(error) = replaced {
        // Nothing was replaced, so only the staged file and the recovery
        // payload have to go.
        let _ = std::fs::remove_file(&staged_target);
        cleanup_recovery(&payload);
        return Err(error);
    }

    // Everything below has replaced the target, so every failure rolls back.
    let committed = (|| -> Result<(), ConfigProfileError> {
        verify_written_source(env, write)?;
        commit_apply(env, write, &locator, now)
    })();
    if let Err(error) = committed {
        let rolled_back = if injected(env, ConfigProfileFaultPoint::RollbackFailure) {
            Err(ConfigProfileError::RollbackFailed)
        } else {
            restore_bytes(write.original.as_deref(), &write.target.path, mode)
        };
        return match rolled_back {
            Ok(()) => {
                cleanup_recovery(&payload);
                Err(error)
            }
            Err(rollback_error) => {
                // The target is stuck in the state the failed apply produced.
                // Recording the recovery point is what makes a manual restore
                // possible at all, so it is written even though the apply
                // failed — and the deployment is still never marked applied.
                let _ = commit_recovery(env, write, &locator, now);
                Err(rollback_error)
            }
        };
    }

    Ok(())
}

/// Reads the target back and checks it is exactly what was written.
fn verify_written_source(
    env: &ConfigProfileWriteEnv<'_>,
    write: &AuthorizedWrite,
) -> Result<(), ConfigProfileError> {
    // `RollbackFailure` also fails here: a rollback only happens after
    // something post-replacement went wrong, so the fault has to produce that
    // first failure before it can make the recovery fail too.
    if injected(env, ConfigProfileFaultPoint::PostWriteVerify)
        || injected(env, ConfigProfileFaultPoint::RollbackFailure)
    {
        return Err(ConfigProfileError::WriteFailed);
    }
    let written = std::fs::read(&write.target.path).map_err(|_| ConfigProfileError::WriteFailed)?;
    if fingerprint(Some(&written)).as_deref() != Some(write.output_fingerprint.as_str()) {
        return Err(ConfigProfileError::WriteFailed);
    }
    Ok(())
}

/// Records the recovery point and the deployment state together.
fn commit_apply(
    env: &ConfigProfileWriteEnv<'_>,
    write: &AuthorizedWrite,
    locator: &str,
    now: i64,
) -> Result<(), ConfigProfileError> {
    if injected(env, ConfigProfileFaultPoint::SqliteCommit) {
        return Err(ConfigProfileError::WriteFailed);
    }
    commit_recovery(env, write, locator, now)?;
    update_deployment_state(
        &env.profile,
        write,
        ASSIGNMENT_STATUS_CLEAN,
        Some(write.output_fingerprint.clone()),
        Some(now),
    )
}

/// Records the single active recovery point of this assignment.
///
/// Kept separate from the deployment update because a failed rollback needs the
/// recovery point recorded without anything claiming the apply succeeded.
fn commit_recovery(
    env: &ConfigProfileWriteEnv<'_>,
    write: &AuthorizedWrite,
    locator: &str,
    now: i64,
) -> Result<(), ConfigProfileError> {
    let store = env.profile.store;
    let previous = store
        .get_config_profile_recovery(&write.profile_id, &write.project_id, write.agent)
        .map_err(|_| ConfigProfileError::WriteFailed)?;

    store
        .upsert_config_profile_recovery(&ConfigProfileRecoveryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            artifact_id: write.profile_id.clone(),
            project_id: write.project_id.clone(),
            agent: write.agent,
            source_id: write.target.source_id.to_string(),
            kind: match write.original {
                Some(_) => HookBackupKind::Bytes,
                None => HookBackupKind::Absent,
            },
            before_hash: fingerprint(write.original.as_deref().map(str::as_bytes))
                .unwrap_or_else(|| ABSENT_FINGERPRINT.to_string()),
            after_hash: write.output_fingerprint.clone(),
            locator: match write.original {
                Some(_) => locator.to_string(),
                // An absent source has no payload file: writing an empty one
                // would restore a zero-byte config instead of removing the file
                // AgentDeck created.
                None => String::new(),
            },
            revision: previous.map(|row| row.revision + 1).unwrap_or(1),
            created_at: now,
        })
        .map_err(|_| ConfigProfileError::WriteFailed)?;
    Ok(())
}

/// Marks the assignment as not reflecting its profile.
///
/// A failure here is deliberately swallowed: the apply already failed and the
/// source is back to its prior state, so a status that could not be written is
/// the lesser of the two problems to report.
fn mark_deployment_failed(env: &ConfigProfileEnv<'_>, write: &AuthorizedWrite) {
    let _ = update_deployment_state(env, write, ASSIGNMENT_STATUS_FAILED, None, None);
}

fn update_deployment_state(
    env: &ConfigProfileEnv<'_>,
    write: &AuthorizedWrite,
    status: &str,
    fingerprint: Option<String>,
    at: Option<i64>,
) -> Result<(), ConfigProfileError> {
    let existing = find_deployment(env, &write.profile_id, &write.project_id, write.agent)?
        .ok_or(ConfigProfileError::WriteFailed)?;
    env.store
        .upsert_deployment(&ArtifactDeploymentRecord {
            status: status.to_string(),
            last_synced_hash: fingerprint.or(existing.last_synced_hash.clone()),
            last_synced_at: at.or(existing.last_synced_at),
            last_error: None,
            ..existing
        })
        .map_err(|_| ConfigProfileError::WriteFailed)
}

/// Removes a recovery payload that no committed metadata points at.
fn cleanup_recovery(payload: &std::path::Path) {
    let _ = std::fs::remove_file(payload);
}

// ---------------------------------------------------------------------------
// Conflict-safe restore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewConfigProfileRestoreRequest {
    pub profile_id: String,
    pub project_id: String,
    pub agent: ConfigAgent,
}

/// The recovery point plus the bytes it holds, resolved together.
///
/// Metadata without its payload is not a usable recovery point, so the two are
/// only ever produced as a pair.
struct ResolvedRecovery {
    record: ConfigProfileRecoveryRecord,
    /// `None` when the recovery point records that the source did not exist.
    bytes: Option<String>,
}

fn resolve_recovery(
    env: &ConfigProfileWriteEnv<'_>,
    profile_id: &str,
    project_id: &str,
    agent: ConfigAgent,
) -> Result<ResolvedRecovery, ConfigProfileError> {
    let record = env
        .profile
        .store
        .get_config_profile_recovery(profile_id, project_id, agent)
        .map_err(|_| ConfigProfileError::WriteFailed)?
        .ok_or(ConfigProfileError::RecoveryNotFound)?;

    let bytes = match record.kind {
        HookBackupKind::Absent => None,
        HookBackupKind::Bytes => {
            let payload = env.recovery_root.join(&record.locator);
            let text = std::fs::read_to_string(&payload)
                // Metadata pointing at a payload that is gone is not a recovery
                // point the user can be offered.
                .map_err(|_| ConfigProfileError::RecoveryNotFound)?;
            Some(text)
        }
    };
    Ok(ResolvedRecovery { record, bytes })
}

/// Builds the diff from the current source back to the recovery point.
///
/// Only allowlisted values are compared, so the raw recovery bytes stay inside
/// this module.
fn restore_diff(
    target: &ConfigProfileTarget,
    current: Option<&str>,
    previous: Option<&str>,
) -> Result<Vec<ConfigProfileDiffEntryDto>, ConfigProfileError> {
    let current = parse_target(target, current)?;
    // A recovery payload that no longer parses is still restorable byte for
    // byte; it just cannot contribute typed values to the diff.
    let previous = parse_target(target, previous).ok();

    let mut diff = Vec::new();
    for key in allowlisted_keys(target.agent) {
        let before = current_value(&current, &key);
        let after = previous.as_ref().and_then(|doc| current_value(doc, &key));
        let status = match (&before, &after) {
            (None, None) => continue,
            (None, Some(_)) => ConfigDiffStatus::Added,
            // Restore is the one operation that can remove a value: the state
            // being restored to genuinely did not have it.
            (Some(_), None) => ConfigDiffStatus::Removed,
            (Some(a), Some(b)) if a == b => ConfigDiffStatus::Same,
            _ => ConfigDiffStatus::Changed,
        };
        if status == ConfigDiffStatus::Same {
            continue;
        }
        diff.push(ConfigProfileDiffEntryDto {
            agent: target.agent,
            canonical_key: key.canonical.to_string(),
            status,
            before,
            after,
        });
    }
    diff.sort_by(|a, b| a.canonical_key.cmp(&b.canonical_key));
    Ok(diff)
}

/// Shows what the latest recovery point would put back.
pub fn preview_restore(
    env: &ConfigProfileWriteEnv<'_>,
    previews: &PreviewStore,
    request: &PreviewConfigProfileRestoreRequest,
    now: i64,
) -> Result<ConfigProfilePreviewDto, ConfigProfileError> {
    let profile = loaded_profile(&env.profile, &request.profile_id)?;
    let recovery = resolve_recovery(
        env,
        &profile.artifact_id,
        &request.project_id,
        request.agent,
    )?;

    let target = resolve_target(&env.profile, &request.project_id, request.agent)?;
    let current = read_target(&target)?;
    let current_fingerprint = fingerprint(current.as_deref().map(str::as_bytes));
    let diff = restore_diff(&target, current.as_deref(), recovery.bytes.as_deref())?;

    let expires_at = now + PREVIEW_TTL_SECONDS;
    let token = previews.issue(BoundPreview {
        operation: ConfigPreviewOperation::Restore,
        profile_id: profile.artifact_id.clone(),
        // The recovery revision, not the profile revision: a restore puts back
        // a saved state and is unaffected by later profile edits.
        profile_revision: recovery.record.revision,
        project_id: request.project_id.clone(),
        agent: request.agent,
        source_id: target.source_id,
        base_fingerprint: current_fingerprint.clone(),
        output_hash: recovery.record.before_hash.clone(),
        expires_at,
    });

    Ok(ConfigProfilePreviewDto {
        token,
        operation: ConfigPreviewOperation::Restore,
        profile_id: profile.artifact_id,
        profile_name: profile.name,
        profile_revision: recovery.record.revision,
        project_id: request.project_id.clone(),
        agent: request.agent,
        source_id: target.source_id.to_string(),
        base_fingerprint: current_fingerprint,
        would_create_file: current.is_none() && recovery.bytes.is_some(),
        would_remove_file: recovery.record.kind == HookBackupKind::Absent,
        diff,
        expires_at,
    })
}

/// Puts the latest recovery point back, saving the current state as the next
/// one first.
///
/// Every step is the apply path's: same write lock, same staged file, same
/// atomic replacement. The difference is only which bytes are being written.
pub fn apply_config_profile_restore(
    env: &ConfigProfileWriteEnv<'_>,
    previews: &PreviewStore,
    request: &ApplyConfigProfileRequest,
    now: i64,
) -> Result<ConfigProfileApplyOutcome, ConfigProfileError> {
    let _guard = CONFIG_PROFILE_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    env.profile.ensure_writable()?;
    ensure_atomic_replacement(env)?;

    let bound = previews.consume(&request.token, now)?;
    if bound.operation != ConfigPreviewOperation::Restore {
        return Err(ConfigProfileError::StalePreview);
    }

    let recovery = resolve_recovery(env, &bound.profile_id, &bound.project_id, bound.agent)?;
    if recovery.record.revision != bound.profile_revision {
        return Err(ConfigProfileError::StalePreview);
    }

    // Resolution refuses a symlink or a special file, so a target swapped since
    // the preview is caught before anything is written.
    let target = resolve_target(&env.profile, &bound.project_id, bound.agent)?;
    if target.source_id != bound.source_id {
        return Err(ConfigProfileError::StalePreview);
    }
    let current = read_target(&target)?;
    if fingerprint(current.as_deref().map(str::as_bytes)) != bound.base_fingerprint {
        return Err(ConfigProfileError::StalePreview);
    }

    let restored_fingerprint = fingerprint(recovery.bytes.as_deref().map(str::as_bytes))
        .unwrap_or_else(|| ABSENT_FINGERPRINT.to_string());
    let write = AuthorizedWrite {
        profile_id: bound.profile_id.clone(),
        project_id: bound.project_id.clone(),
        agent: bound.agent,
        target,
        // The current state becomes the next recovery point, which is what
        // makes the undo itself undoable.
        original: current,
        document_text: recovery.bytes.clone().unwrap_or_default(),
        output_fingerprint: restored_fingerprint.clone(),
    };

    let created_file = write.original.is_none();
    let removing = recovery.record.kind == HookBackupKind::Absent;
    match perform_restore(env, &write, removing, now) {
        Ok(()) => Ok(ConfigProfileApplyOutcome {
            profile_id: write.profile_id,
            project_id: write.project_id,
            agent: write.agent,
            fingerprint: restored_fingerprint,
            created_file,
        }),
        Err(error) => {
            mark_deployment_failed(&env.profile, &write);
            Err(error)
        }
    }
}

/// The restore equivalent of `perform_write`: save the current state, then put
/// the previous one back atomically.
fn perform_restore(
    env: &ConfigProfileWriteEnv<'_>,
    write: &AuthorizedWrite,
    removing: bool,
    now: i64,
) -> Result<(), ConfigProfileError> {
    let locator = recovery_locator(&write.profile_id, &write.project_id, write.agent);
    let payload = env.recovery_root.join(&locator);
    let payload_dir = payload.parent().ok_or(ConfigProfileError::WriteFailed)?;
    let mode = mode_of(&write.target.path).unwrap_or(0o600);

    // The current state is staged as the next recovery point before the target
    // changes, so a failure below still leaves both states reachable.
    let staged_payload = payload.with_extension("staged");
    if let Some(current) = write.original.as_deref() {
        create_private_dir(payload_dir)?;
        write_synced(&staged_payload, current.as_bytes(), 0o600)
            .map_err(|_| ConfigProfileError::WriteFailed)?;
    }

    let parent = write
        .target
        .path
        .parent()
        .ok_or(ConfigProfileError::WriteFailed)?
        .to_path_buf();
    let replaced = if removing {
        // `remove_file` does not follow a link, and resolution already refused
        // one, so this cannot delete something outside the Project.
        match std::fs::remove_file(&write.target.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(ConfigProfileError::WriteFailed),
        }
    } else {
        std::fs::create_dir_all(&parent).map_err(|_| ConfigProfileError::WriteFailed)?;
        let staged_target = staged_path(&write.target.path);
        let outcome = (|| -> Result<(), ConfigProfileError> {
            write_synced(&staged_target, write.document_text.as_bytes(), mode)
                .map_err(|_| ConfigProfileError::WriteFailed)?;
            std::fs::rename(&staged_target, &write.target.path)
                .map_err(|_| ConfigProfileError::WriteFailed)?;
            Ok(())
        })();
        if outcome.is_err() {
            let _ = std::fs::remove_file(&staged_target);
        }
        outcome
    };
    let _ = sync_dir(&parent);

    if let Err(error) = replaced {
        let _ = std::fs::remove_file(&staged_payload);
        return Err(error);
    }

    // Promote the staged payload and record it as the new recovery point.
    let committed = (|| -> Result<(), ConfigProfileError> {
        if write.original.is_some() {
            std::fs::rename(&staged_payload, &payload)
                .map_err(|_| ConfigProfileError::WriteFailed)?;
            let _ = sync_dir(payload_dir);
        } else {
            cleanup_recovery(&payload);
        }
        commit_recovery(env, write, &locator, now)?;
        update_deployment_state(
            &env.profile,
            write,
            ASSIGNMENT_STATUS_CLEAN,
            Some(write.output_fingerprint.clone()),
            Some(now),
        )
    })();
    if let Err(error) = committed {
        let _ = std::fs::remove_file(&staged_payload);
        // Put the target back the way the restore found it.
        restore_bytes(write.original.as_deref(), &write.target.path, mode)?;
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill_store::ProjectRecord;
    use tempfile::TempDir;

    fn store(dir: &TempDir) -> SkillStore {
        SkillStore::new(&dir.path().join("test.db")).unwrap()
    }

    fn register_project(store: &SkillStore, id: &str, root: &std::path::Path) {
        store
            .insert_project(&ProjectRecord {
                id: id.to_string(),
                name: id.to_string(),
                path: root.display().to_string(),
                workspace_type: "project".to_string(),
                linked_agent_key: None,
                linked_agent_name: None,
                disabled_path: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
    }

    fn entry(agent: ConfigAgent, key: &str, value: ConfigValueDto) -> ConfigProfileEntryDto {
        ConfigProfileEntryDto {
            agent,
            canonical_key: key.to_string(),
            value,
        }
    }

    /// The profile from the spec example, which mixes both Agents and all three
    /// scalar types.
    fn development_entries() -> Vec<ConfigProfileEntryDto> {
        vec![
            entry(
                ConfigAgent::Codex,
                "sandbox_mode",
                ConfigValueDto::String("read-only".to_string()),
            ),
            entry(
                ConfigAgent::Codex,
                "model_reasoning_effort",
                ConfigValueDto::String("high".to_string()),
            ),
            entry(
                ConfigAgent::ClaudeCode,
                "always_thinking_enabled",
                ConfigValueDto::Boolean(true),
            ),
            entry(
                ConfigAgent::ClaudeCode,
                "cleanup_period_days",
                ConfigValueDto::Integer(20),
            ),
        ]
    }

    fn create(
        env: &ConfigProfileEnv<'_>,
        name: &str,
        entries: Vec<ConfigProfileEntryDto>,
    ) -> Result<ConfigProfileDto, ConfigProfileError> {
        create_profile(
            env,
            &CreateConfigProfileRequest {
                name: name.to_string(),
                entries,
            },
            100,
        )
    }

    // Requirement: Profiles persist only exact typed non-sensitive settings
    // Scenario: Valid mixed-Agent profile is stored as typed entries
    #[test]
    fn profile_crud_create_stores_typed_entries_at_revision_one() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };

        let profile = create(&env, "Development", development_entries()).unwrap();

        assert_eq!(profile.revision, 1);
        assert_eq!(profile.name, "Development");
        assert_eq!(profile.entries.len(), 4);

        let stored = list_profiles(&env).unwrap();
        assert_eq!(stored.len(), 1);
        let mut keys: Vec<(ConfigAgent, String, ConfigValueDto)> = stored[0]
            .entries
            .iter()
            .map(|e| (e.agent, e.canonical_key.clone(), e.value.clone()))
            .collect();
        keys.sort_by(|a, b| (a.0, a.1.as_str()).cmp(&(b.0, b.1.as_str())));
        assert_eq!(
            keys,
            vec![
                (
                    ConfigAgent::Codex,
                    "model_reasoning_effort".to_string(),
                    ConfigValueDto::String("high".to_string())
                ),
                (
                    ConfigAgent::Codex,
                    "sandbox_mode".to_string(),
                    ConfigValueDto::String("read-only".to_string())
                ),
                (
                    ConfigAgent::ClaudeCode,
                    "always_thinking_enabled".to_string(),
                    ConfigValueDto::Boolean(true)
                ),
                (
                    ConfigAgent::ClaudeCode,
                    "cleanup_period_days".to_string(),
                    ConfigValueDto::Integer(20)
                ),
            ]
        );
    }

    // Requirement: Profiles persist only exact typed non-sensitive settings
    // Scenario: Unknown or wrong-type entry is rejected atomically
    #[test]
    fn profile_crud_rejects_unknown_or_wrong_type_entry_without_writing() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };

        let rejected = [
            // A key outside the allowlist.
            vec![entry(
                ConfigAgent::Codex,
                "api_key",
                ConfigValueDto::String("sk-live-1".to_string()),
            )],
            // An allowlisted key for the wrong Agent.
            vec![entry(
                ConfigAgent::Codex,
                "cleanup_period_days",
                ConfigValueDto::Integer(20),
            )],
            // The right key with the wrong scalar type.
            vec![entry(
                ConfigAgent::ClaudeCode,
                "cleanup_period_days",
                ConfigValueDto::String("20".to_string()),
            )],
            vec![entry(
                ConfigAgent::Codex,
                "web_search",
                ConfigValueDto::String("true".to_string()),
            )],
            // The same key twice, which no single source could satisfy.
            vec![
                entry(
                    ConfigAgent::Codex,
                    "model",
                    ConfigValueDto::String("gpt-5".to_string()),
                ),
                entry(
                    ConfigAgent::Codex,
                    "model",
                    ConfigValueDto::String("gpt-5.1".to_string()),
                ),
            ],
        ];

        for entries in rejected {
            let error = create(&env, "Rejected", entries.clone()).unwrap_err();
            assert_eq!(
                error,
                ConfigProfileError::InvalidProfileEntry,
                "{entries:?}"
            );
        }
        // An empty name is equally not a profile.
        assert_eq!(
            create(&env, "   ", Vec::new()).unwrap_err(),
            ConfigProfileError::InvalidProfileEntry
        );

        assert!(list_profiles(&env).unwrap().is_empty());
    }

    // Requirement: Profile CRUD is revisioned and transactionally consistent
    // Scenario: Successful update replaces the entry set once
    #[test]
    fn profile_crud_update_replaces_the_entry_set_and_increments_once() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };
        let mut profile = create(&env, "Development", development_entries()).unwrap();

        // Three saves take the profile from revision 1 to revision 4, one step
        // per save, so the spec's revision 3 → 4 case is exercised as the last.
        for (round, value) in ["gpt-5", "gpt-5.1", "gpt-5.2"].into_iter().enumerate() {
            let expected = profile.revision;
            profile = update_profile(
                &env,
                &UpdateConfigProfileRequest {
                    profile_id: profile.id.clone(),
                    expected_revision: expected,
                    name: "Development".to_string(),
                    entries: vec![entry(
                        ConfigAgent::Codex,
                        "model",
                        ConfigValueDto::String(value.to_string()),
                    )],
                },
                200 + round as i64,
            )
            .unwrap();
            assert_eq!(profile.revision, expected + 1);
        }

        assert_eq!(profile.revision, 4);
        let stored = &list_profiles(&env).unwrap()[0];
        // The complete set was replaced, not merged with the original four.
        assert_eq!(stored.entries.len(), 1);
        assert_eq!(
            stored.entries[0].value,
            ConfigValueDto::String("gpt-5.2".to_string())
        );
        assert_eq!(stored.revision, 4);
    }

    /// A save that changes nothing must not advance the revision: every
    /// outstanding apply preview is bound to it.
    #[test]
    fn profile_crud_update_without_a_change_keeps_the_revision() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };
        let profile = create(&env, "Development", development_entries()).unwrap();

        // Same entries, reversed, which is the same set.
        let mut reordered = development_entries();
        reordered.reverse();
        let saved = update_profile(
            &env,
            &UpdateConfigProfileRequest {
                profile_id: profile.id.clone(),
                expected_revision: 1,
                name: "Development".to_string(),
                entries: reordered,
            },
            200,
        )
        .unwrap();

        assert_eq!(saved.revision, 1);
    }

    // Requirement: Profile CRUD is revisioned and transactionally consistent
    // Scenario: Stale profile editor is rejected
    #[test]
    fn profile_crud_stale_editor_changes_nothing() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };
        let profile = create(&env, "Development", development_entries()).unwrap();
        let advanced = update_profile(
            &env,
            &UpdateConfigProfileRequest {
                profile_id: profile.id.clone(),
                expected_revision: 1,
                name: "Development".to_string(),
                entries: vec![entry(
                    ConfigAgent::Codex,
                    "model",
                    ConfigValueDto::String("gpt-5".to_string()),
                )],
            },
            200,
        )
        .unwrap();
        assert_eq!(advanced.revision, 2);
        let before = list_profiles(&env).unwrap();

        let error = update_profile(
            &env,
            &UpdateConfigProfileRequest {
                profile_id: profile.id.clone(),
                // The revision this editor was opened on, now superseded.
                expected_revision: 1,
                name: "Renamed".to_string(),
                entries: Vec::new(),
            },
            300,
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::StaleProfile);
        assert_eq!(list_profiles(&env).unwrap(), before);
    }

    #[test]
    fn profile_crud_update_of_a_missing_profile_is_not_found() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };

        let error = update_profile(
            &env,
            &UpdateConfigProfileRequest {
                profile_id: "no-such-profile".to_string(),
                expected_revision: 1,
                name: "Development".to_string(),
                entries: Vec::new(),
            },
            200,
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::ProfileNotFound);
    }

    #[test]
    fn profile_crud_delete_removes_an_unassigned_profile() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };
        let profile = create(&env, "Development", development_entries()).unwrap();

        delete_profile(
            &env,
            &DeleteConfigProfileRequest {
                profile_id: profile.id.clone(),
            },
        )
        .unwrap();

        assert!(list_profiles(&env).unwrap().is_empty());
        assert_eq!(
            delete_profile(
                &env,
                &DeleteConfigProfileRequest {
                    profile_id: profile.id,
                },
            )
            .unwrap_err(),
            ConfigProfileError::ProfileNotFound
        );
    }

    /// Every mutation is refused before it can touch SQLite when the Library is
    /// offline; reading the profile list is not a mutation and stays available.
    #[test]
    fn profile_crud_offline_library_blocks_mutation_but_not_reads() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        let online = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };
        let profile = create(&online, "Development", development_entries()).unwrap();

        let offline = ConfigProfileEnv {
            store: &store,
            library_online: false,
        };
        assert_eq!(
            create(&offline, "Blocked", Vec::new()).unwrap_err(),
            ConfigProfileError::LibraryOffline
        );
        assert_eq!(
            update_profile(
                &offline,
                &UpdateConfigProfileRequest {
                    profile_id: profile.id.clone(),
                    expected_revision: 1,
                    name: "Blocked".to_string(),
                    entries: Vec::new(),
                },
                200,
            )
            .unwrap_err(),
            ConfigProfileError::LibraryOffline
        );
        assert_eq!(
            delete_profile(
                &offline,
                &DeleteConfigProfileRequest {
                    profile_id: profile.id.clone(),
                },
            )
            .unwrap_err(),
            ConfigProfileError::LibraryOffline
        );

        let listed = list_profiles(&offline).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].revision, 1);
    }

    // ── Assignments ──

    /// A profile, two registered Projects and an env, which is the fixture
    /// every assignment case starts from.
    struct Fixture {
        _dir: TempDir,
        _roots: TempDir,
        store: SkillStore,
        profile_id: String,
        alpha: std::path::PathBuf,
        beta: std::path::PathBuf,
    }

    fn assignment_fixture() -> Fixture {
        let dir = TempDir::new().unwrap();
        let roots = TempDir::new().unwrap();
        let store = store(&dir);
        let alpha = roots.path().join("alpha");
        let beta = roots.path().join("beta");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();
        register_project(&store, "alpha", &alpha);
        register_project(&store, "beta", &beta);

        let profile_id = {
            let env = ConfigProfileEnv {
                store: &store,
                library_online: true,
            };
            create(&env, "Development", development_entries())
                .unwrap()
                .id
        };
        Fixture {
            _dir: dir,
            _roots: roots,
            store,
            profile_id,
            alpha,
            beta,
        }
    }

    impl Fixture {
        fn env(&self) -> ConfigProfileEnv<'_> {
            ConfigProfileEnv {
                store: &self.store,
                library_online: true,
            }
        }

        fn assign(
            &self,
            project_id: &str,
            agent: ConfigAgent,
        ) -> Result<ConfigProfileAssignmentDto, ConfigProfileError> {
            set_assignment(
                &self.env(),
                &SetConfigProfileAssignmentRequest {
                    profile_id: self.profile_id.clone(),
                    project_id: project_id.to_string(),
                    agent,
                },
            )
        }

        fn deployment_count(&self) -> i64 {
            self.store.get_all_deployments().unwrap().len() as i64
        }

        /// Every file under both Project roots, so a source write anywhere is
        /// visible as a difference.
        fn source_snapshot(&self) -> Vec<String> {
            let mut files = Vec::new();
            for root in [&self.alpha, &self.beta] {
                for entry in walkdir::WalkDir::new(root)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                {
                    let bytes = std::fs::read(entry.path()).unwrap_or_default();
                    files.push(format!("{}|{}", entry.path().display(), bytes.len()));
                }
            }
            files.sort();
            files
        }
    }

    // Requirement: Assignments reuse canonical Project deployments
    // Scenario: One profile is assigned to two Projects and both Agents
    #[test]
    fn assignment_integrity_one_profile_spans_two_projects_and_both_agents() {
        let fixture = assignment_fixture();
        let before = fixture.source_snapshot();

        for (project, agent) in [
            ("alpha", ConfigAgent::Codex),
            ("alpha", ConfigAgent::ClaudeCode),
            ("beta", ConfigAgent::Codex),
            ("beta", ConfigAgent::ClaudeCode),
        ] {
            let assignment = fixture.assign(project, agent).unwrap();
            assert_eq!(assignment.profile_id, fixture.profile_id);
            assert_eq!(assignment.project_id, project);
            assert_eq!(assignment.agent, agent);
            assert!(!assignment.has_recovery_point);
            assert_eq!(assignment.last_applied_fingerprint, None);
        }

        assert_eq!(fixture.deployment_count(), 4);
        let listed = list_assignments(&fixture.env(), Some(&fixture.profile_id)).unwrap();
        assert_eq!(listed.len(), 4);
        // Each identity names the same profile Artifact and its own tuple.
        let mut tuples: Vec<(String, ConfigAgent)> = listed
            .iter()
            .map(|a| (a.project_id.clone(), a.agent))
            .collect();
        tuples.sort_by(|a, b| (a.0.as_str(), a.1).cmp(&(b.0.as_str(), b.1)));
        assert_eq!(
            tuples,
            vec![
                ("alpha".to_string(), ConfigAgent::Codex),
                ("alpha".to_string(), ConfigAgent::ClaudeCode),
                ("beta".to_string(), ConfigAgent::Codex),
                ("beta".to_string(), ConfigAgent::ClaudeCode),
            ]
        );
        assert!(listed.iter().all(|a| a.profile_id == fixture.profile_id));

        // Assignment is metadata: nothing under either Project root changed.
        assert_eq!(fixture.source_snapshot(), before);
    }

    /// The same tuple assigned twice is one identity, not two.
    #[test]
    fn assignment_integrity_repeated_assignment_stays_one_identity() {
        let fixture = assignment_fixture();

        let first = fixture.assign("alpha", ConfigAgent::Codex).unwrap();
        let second = fixture.assign("alpha", ConfigAgent::Codex).unwrap();

        assert_eq!(first.source_id, second.source_id);
        assert_eq!(fixture.deployment_count(), 1);
    }

    /// The two Agents of one Project resolve to their own fixed sources, which
    /// is what keeps an apply from writing the wrong file.
    #[test]
    fn assignment_integrity_agents_carry_distinct_fixed_source_ids() {
        let fixture = assignment_fixture();

        let codex = fixture.assign("alpha", ConfigAgent::Codex).unwrap();
        let claude = fixture.assign("alpha", ConfigAgent::ClaudeCode).unwrap();

        assert_eq!(codex.source_id, "codex:project:config-toml");
        assert_eq!(claude.source_id, "claude_code:project:settings-json");
    }

    // Requirement: Assignments reuse canonical Project deployments
    // Scenario: Unknown Project assignment is rejected
    #[test]
    fn assignment_integrity_unknown_project_creates_no_deployment() {
        let fixture = assignment_fixture();
        let before = fixture.source_snapshot();

        let error = fixture.assign("gamma", ConfigAgent::Codex).unwrap_err();

        assert_eq!(error, ConfigProfileError::ProjectNotFound);
        assert_eq!(fixture.deployment_count(), 0);
        assert_eq!(fixture.source_snapshot(), before);
    }

    #[test]
    fn assignment_integrity_unknown_profile_creates_no_deployment() {
        let fixture = assignment_fixture();

        let error = set_assignment(
            &fixture.env(),
            &SetConfigProfileAssignmentRequest {
                profile_id: "no-such-profile".to_string(),
                project_id: "alpha".to_string(),
                agent: ConfigAgent::Codex,
            },
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::ProfileNotFound);
        assert_eq!(fixture.deployment_count(), 0);
    }

    // Requirement: Assignments reuse canonical Project deployments
    // Scenario: Removing an assignment does not mutate its source
    #[test]
    fn assignment_integrity_removal_leaves_project_bytes_unchanged() {
        let fixture = assignment_fixture();
        fixture.assign("alpha", ConfigAgent::Codex).unwrap();
        fixture.assign("beta", ConfigAgent::Codex).unwrap();
        // A source the user already had, which removal must not touch.
        let existing = fixture.alpha.join(".codex");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("config.toml"), b"model = \"gpt-5\"\n").unwrap();
        let before = fixture.source_snapshot();

        remove_assignment(
            &fixture.env(),
            &RemoveConfigProfileAssignmentRequest {
                profile_id: fixture.profile_id.clone(),
                project_id: "alpha".to_string(),
                agent: ConfigAgent::Codex,
            },
        )
        .unwrap();

        // Only the matching identity went.
        assert_eq!(fixture.deployment_count(), 1);
        let remaining = list_assignments(&fixture.env(), Some(&fixture.profile_id)).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].project_id, "beta");
        assert_eq!(fixture.source_snapshot(), before);
    }

    /// A recovery point is the only way back from a write this assignment made,
    /// so the assignment cannot be removed while one exists.
    #[test]
    fn assignment_integrity_protected_recovery_blocks_removal() {
        let fixture = assignment_fixture();
        fixture.assign("alpha", ConfigAgent::Codex).unwrap();
        fixture
            .store
            .upsert_config_profile_recovery(
                &crate::core::skill_store::ConfigProfileRecoveryRecord {
                    id: "rec-1".to_string(),
                    artifact_id: fixture.profile_id.clone(),
                    project_id: "alpha".to_string(),
                    agent: ConfigAgent::Codex,
                    source_id: "codex:project:config-toml".to_string(),
                    kind: crate::core::artifact::HookBackupKind::Bytes,
                    before_hash: "aaa".to_string(),
                    after_hash: "bbb".to_string(),
                    locator: "alpha/codex/latest".to_string(),
                    revision: 1,
                    created_at: 100,
                },
            )
            .unwrap();

        let error = remove_assignment(
            &fixture.env(),
            &RemoveConfigProfileAssignmentRequest {
                profile_id: fixture.profile_id.clone(),
                project_id: "alpha".to_string(),
                agent: ConfigAgent::Codex,
            },
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::ProfileInUse);
        assert_eq!(fixture.deployment_count(), 1);
        let listed = list_assignments(&fixture.env(), Some(&fixture.profile_id)).unwrap();
        assert!(listed[0].has_recovery_point);
    }

    // Requirement: Profile CRUD is revisioned and transactionally consistent
    // Scenario: In-use profile cannot be deleted
    #[test]
    fn assignment_integrity_assigned_profile_cannot_be_deleted() {
        let fixture = assignment_fixture();
        fixture.assign("alpha", ConfigAgent::Codex).unwrap();

        let error = delete_profile(
            &fixture.env(),
            &DeleteConfigProfileRequest {
                profile_id: fixture.profile_id.clone(),
            },
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::ProfileInUse);
        assert_eq!(fixture.deployment_count(), 1);
        assert_eq!(list_profiles(&fixture.env()).unwrap().len(), 1);
    }

    #[test]
    fn assignment_integrity_offline_library_blocks_assignment_mutation() {
        let fixture = assignment_fixture();
        let offline = ConfigProfileEnv {
            store: &fixture.store,
            library_online: false,
        };

        assert_eq!(
            set_assignment(
                &offline,
                &SetConfigProfileAssignmentRequest {
                    profile_id: fixture.profile_id.clone(),
                    project_id: "alpha".to_string(),
                    agent: ConfigAgent::Codex,
                },
            )
            .unwrap_err(),
            ConfigProfileError::LibraryOffline
        );
        assert_eq!(
            remove_assignment(
                &offline,
                &RemoveConfigProfileAssignmentRequest {
                    profile_id: fixture.profile_id.clone(),
                    project_id: "alpha".to_string(),
                    agent: ConfigAgent::Codex,
                },
            )
            .unwrap_err(),
            ConfigProfileError::LibraryOffline
        );
        assert_eq!(fixture.deployment_count(), 0);
    }

    // ── Fixed project targets ──

    fn write_source(path: &std::path::Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    // Requirement: Mutation resolves only fixed Project sources
    // Scenario: Codex and Claude assignments resolve distinct fixed targets
    #[test]
    fn fixed_project_targets_are_exactly_the_two_project_sources() {
        let fixture = assignment_fixture();
        let env = fixture.env();

        let codex = resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap();
        let claude = resolve_target(&env, "alpha", ConfigAgent::ClaudeCode).unwrap();

        assert_eq!(codex.path, fixture.alpha.join(".codex").join("config.toml"));
        assert_eq!(
            claude.path,
            fixture.alpha.join(".claude").join("settings.json")
        );
        assert_eq!(codex.source_id, "codex:project:config-toml");
        assert_eq!(claude.source_id, "claude_code:project:settings-json");
        // A second Project resolves under its own root, never the first one's.
        let beta = resolve_target(&env, "beta", ConfigAgent::Codex).unwrap();
        assert_eq!(beta.path, fixture.beta.join(".codex").join("config.toml"));
    }

    /// The user sources and the Claude project-local source are readable on the
    /// inspection path and must stay unreachable here: no Agent resolves to
    /// them, so no request can name one.
    #[test]
    fn fixed_project_targets_exclude_user_and_project_local_sources() {
        let fixture = assignment_fixture();
        let env = fixture.env();

        let mut resolved: Vec<std::path::PathBuf> = Vec::new();
        for agent in [ConfigAgent::Codex, ConfigAgent::ClaudeCode] {
            for project in ["alpha", "beta"] {
                resolved.push(resolve_target(&env, project, agent).unwrap().path);
            }
        }

        for path in &resolved {
            let text = path.display().to_string();
            assert!(
                !text.contains("settings.local.json"),
                "project-local source is not writable: {text}"
            );
        }
        assert_eq!(resolved.len(), 4);
        // Every resolved path sits under a registered Project root.
        assert!(resolved
            .iter()
            .all(|path| path.starts_with(&fixture.alpha) || path.starts_with(&fixture.beta)));
    }

    // Requirement: Mutation resolves only fixed Project sources
    // Scenario: Missing fixed target is a create candidate
    #[test]
    fn fixed_project_targets_missing_source_is_a_create_candidate() {
        let fixture = assignment_fixture();
        let env = fixture.env();

        let target = resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap();

        assert_eq!(target.state, ConfigTargetState::Missing);
        assert_eq!(read_target(&target).unwrap(), None);
        // Resolution created neither the directory nor the file.
        assert!(!fixture.alpha.join(".codex").exists());
    }

    #[test]
    fn fixed_project_targets_existing_source_is_read_whole() {
        let fixture = assignment_fixture();
        let env = fixture.env();
        write_source(
            &fixture.alpha.join(".codex").join("config.toml"),
            b"# comment\nmodel = \"gpt-5\"\n",
        );

        let target = resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap();

        assert_eq!(target.state, ConfigTargetState::Present);
        assert_eq!(
            read_target(&target).unwrap(),
            Some("# comment\nmodel = \"gpt-5\"\n".to_string())
        );
    }

    // Requirement: Mutation resolves only fixed Project sources
    // Scenario: Unknown Project assignment is rejected
    #[test]
    fn fixed_project_targets_unknown_project_is_rejected() {
        let fixture = assignment_fixture();
        let env = fixture.env();

        for agent in [ConfigAgent::Codex, ConfigAgent::ClaudeCode] {
            assert_eq!(
                resolve_target(&env, "gamma", agent).unwrap_err(),
                ConfigProfileError::ProjectNotFound
            );
        }
    }

    /// A Project record whose root has gone is refused rather than recreated:
    /// resolution never makes a directory.
    #[test]
    fn fixed_project_targets_missing_project_root_is_invalid() {
        let dir = TempDir::new().unwrap();
        let store = store(&dir);
        register_project(&store, "ghost", std::path::Path::new("/nonexistent/ghost"));
        let env = ConfigProfileEnv {
            store: &store,
            library_online: true,
        };

        assert_eq!(
            resolve_target(&env, "ghost", ConfigAgent::Codex).unwrap_err(),
            ConfigProfileError::SourceInvalid
        );
        assert!(!std::path::Path::new("/nonexistent/ghost").exists());
    }

    // Requirement: Mutation resolves only fixed Project sources
    // Scenario: Symlink or special target is rejected before preview
    #[test]
    #[cfg(unix)]
    fn fixed_project_targets_symlink_is_rejected_without_reading_the_link() {
        let fixture = assignment_fixture();
        let env = fixture.env();
        // The link points at a real file outside the Project, which is exactly
        // the write that must never happen.
        let outside = fixture.beta.join("secret.toml");
        write_source(&outside, b"model = \"leaked\"\n");
        let target_path = fixture.alpha.join(".codex").join("config.toml");
        std::fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &target_path).unwrap();

        let error = resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap_err();

        assert_eq!(error, ConfigProfileError::UnsupportedSymlink);
        // The link target is untouched and was never adopted as the source.
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"model = \"leaked\"\n".to_vec()
        );
    }

    #[test]
    #[cfg(unix)]
    fn fixed_project_targets_special_file_is_rejected() {
        let fixture = assignment_fixture();
        let env = fixture.env();
        // A directory where the config file belongs is the portable stand-in
        // for a non-regular file.
        std::fs::create_dir_all(fixture.alpha.join(".codex").join("config.toml")).unwrap();

        assert_eq!(
            resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap_err(),
            ConfigProfileError::SourceInvalid
        );
    }

    // Requirement: Mutation resolves only fixed Project sources
    // Scenario: Invalid or oversized target is not repaired
    #[test]
    fn fixed_project_targets_oversized_source_is_refused_before_reading() {
        let fixture = assignment_fixture();
        let env = fixture.env();
        let path = fixture.alpha.join(".codex").join("config.toml");
        let oversized = vec![b'#'; (super::MAX_SOURCE_BYTES + 1) as usize];
        write_source(&path, &oversized);

        let target = resolve_target(&env, "alpha", ConfigAgent::Codex).unwrap();
        let error = read_target(&target).unwrap_err();

        assert_eq!(error, ConfigProfileError::TooLarge);
        // The source is exactly as it was: nothing repaired or truncated it.
        assert_eq!(std::fs::read(&path).unwrap().len(), oversized.len());
    }

    #[test]
    fn fixed_project_targets_invalid_document_is_not_repaired() {
        let fixture = assignment_fixture();
        let env = fixture.env();
        let codex_path = fixture.alpha.join(".codex").join("config.toml");
        let claude_path = fixture.alpha.join(".claude").join("settings.json");
        write_source(&codex_path, b"model = \n");
        write_source(&claude_path, b"{ \"model\": }");

        for agent in [ConfigAgent::Codex, ConfigAgent::ClaudeCode] {
            let target = resolve_target(&env, "alpha", agent).unwrap();
            let text = read_target(&target).unwrap().unwrap();
            assert_eq!(
                parse_target(&target, Some(&text)).unwrap_err(),
                ConfigProfileError::SourceInvalid
            );
        }

        assert_eq!(std::fs::read(&codex_path).unwrap(), b"model = \n".to_vec());
        assert_eq!(
            std::fs::read(&claude_path).unwrap(),
            b"{ \"model\": }".to_vec()
        );
    }

    // ── Preview authority ──

    /// A fixture with one assignment already in place, which is the state every
    /// preview case starts from.
    fn preview_fixture() -> Fixture {
        let fixture = assignment_fixture();
        fixture.assign("alpha", ConfigAgent::Codex).unwrap();
        fixture.assign("alpha", ConfigAgent::ClaudeCode).unwrap();
        fixture
    }

    impl Fixture {
        fn preview(
            &self,
            previews: &PreviewStore,
            agent: ConfigAgent,
        ) -> Result<ConfigProfilePreviewDto, ConfigProfileError> {
            preview_apply(
                &self.env(),
                previews,
                &PreviewConfigProfileApplyRequest {
                    profile_id: self.profile_id.clone(),
                    project_id: "alpha".to_string(),
                    agent,
                },
                1_000,
            )
        }

        fn codex_path(&self) -> std::path::PathBuf {
            self.alpha.join(".codex").join("config.toml")
        }
    }

    /// Sets the profile to exactly one Codex entry, returning the new revision.
    fn set_single_codex_model(fixture: &Fixture, value: &str) -> i64 {
        let current = list_profiles(&fixture.env()).unwrap()[0].revision;
        update_profile(
            &fixture.env(),
            &UpdateConfigProfileRequest {
                profile_id: fixture.profile_id.clone(),
                expected_revision: current,
                name: "Development".to_string(),
                entries: vec![entry(
                    ConfigAgent::Codex,
                    "model",
                    ConfigValueDto::String(value.to_string()),
                )],
            },
            200,
        )
        .unwrap()
        .revision
    }

    // Requirement: Apply requires an exact single-use typed preview
    // Scenario: Preview shows only explicit allowlisted changes
    #[test]
    fn preview_authority_reports_one_typed_change_and_no_source_content() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(
            &fixture.codex_path(),
            b"# keep me\nmodel = \"gpt-5\"\n[mcp.demo]\ncommand = \"secret-tool\"\n",
        );
        let revision = set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(preview.profile_revision, revision);
        assert_eq!(preview.source_id, "codex:project:config-toml");
        assert_eq!(preview.diff.len(), 1);
        let change = &preview.diff[0];
        assert_eq!(change.canonical_key, "model");
        assert_eq!(change.status, ConfigDiffStatus::Changed);
        assert_eq!(
            change.before,
            Some(ConfigValueDto::String("gpt-5".to_string()))
        );
        assert_eq!(
            change.after,
            Some(ConfigValueDto::String("gpt-5.1".to_string()))
        );
        assert!(preview.base_fingerprint.is_some());

        // Nothing in the serialized preview reproduces the source: no unknown
        // key, no unknown value, no path, no raw document.
        let json = serde_json::to_string(&preview).unwrap();
        for forbidden in ["keep me", "mcp", "secret-tool", "command", ".codex"] {
            assert!(
                !json.contains(forbidden),
                "preview leaked {forbidden}: {json}"
            );
        }
    }

    // Requirement: Apply requires an exact single-use typed preview
    // Scenario: Profile omission preserves the existing setting
    #[test]
    fn preview_authority_omitted_key_is_absent_from_the_diff() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(
            &fixture.codex_path(),
            b"sandbox_mode = \"danger-full-access\"\nmodel = \"gpt-5\"\n",
        );
        // The profile carries `model` only, so `sandbox_mode` is not its
        // business — absence is not a removal.
        set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(preview.diff.len(), 1);
        assert_eq!(preview.diff[0].canonical_key, "model");
        assert!(preview
            .diff
            .iter()
            .all(|entry| entry.status != ConfigDiffStatus::Removed));
    }

    #[test]
    fn preview_authority_missing_target_is_a_create_preview() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(preview.base_fingerprint, None);
        assert!(preview.would_create_file);
        assert_eq!(preview.diff.len(), 1);
        assert_eq!(preview.diff[0].status, ConfigDiffStatus::Added);
        // Previewing created nothing.
        assert!(!fixture.alpha.join(".codex").exists());
    }

    #[test]
    fn preview_authority_unchanged_value_is_reported_as_same() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), b"model = \"gpt-5.1\"\n");
        set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(preview.diff.len(), 1);
        assert_eq!(preview.diff[0].status, ConfigDiffStatus::Same);
    }

    /// The token is the whole apply request, so consuming it must yield every
    /// bound input the apply revalidates.
    #[test]
    fn preview_authority_token_binds_every_identity() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), b"model = \"gpt-5\"\n");
        let revision = set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();
        let bound = previews.consume(&preview.token, 1_000).unwrap();

        assert_eq!(bound.profile_id, fixture.profile_id);
        assert_eq!(bound.profile_revision, revision);
        assert_eq!(bound.project_id, "alpha");
        assert_eq!(bound.agent, ConfigAgent::Codex);
        assert_eq!(bound.source_id, "codex:project:config-toml");
        assert!(bound.base_fingerprint.is_some());
        assert!(!bound.output_hash.is_empty());
    }

    // Requirement: Apply requires an exact single-use typed preview
    // Scenario: Token is expired or replayed
    #[test]
    fn preview_authority_token_is_single_use() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert!(previews.consume(&preview.token, 1_000).is_ok());

        assert_eq!(
            previews.consume(&preview.token, 1_000).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn preview_authority_token_expires() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(
            previews
                .consume(&preview.token, preview.expires_at + 1)
                .unwrap_err(),
            ConfigProfileError::PreviewExpired
        );
        // An expired token is gone, not merely refused once.
        assert_eq!(
            previews.consume(&preview.token, 1_000).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn preview_authority_unknown_token_is_stale() {
        let previews = PreviewStore::new();

        assert_eq!(
            previews.consume("not-a-token", 1_000).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn preview_authority_requires_an_existing_assignment() {
        let fixture = assignment_fixture();
        let previews = PreviewStore::new();

        // No assignment has been made for this Project and Agent.
        assert_eq!(
            fixture.preview(&previews, ConfigAgent::Codex).unwrap_err(),
            ConfigProfileError::ProfileNotFound
        );
    }

    #[test]
    fn preview_authority_refuses_invalid_and_oversized_sources_without_a_token() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        write_source(&fixture.codex_path(), b"model = \n");

        assert_eq!(
            fixture.preview(&previews, ConfigAgent::Codex).unwrap_err(),
            ConfigProfileError::SourceInvalid
        );

        write_source(
            &fixture.codex_path(),
            &vec![b'#'; (super::MAX_SOURCE_BYTES + 1) as usize],
        );
        assert_eq!(
            fixture.preview(&previews, ConfigAgent::Codex).unwrap_err(),
            ConfigProfileError::TooLarge
        );

        assert!(previews.is_empty());
    }

    fn authorize(
        fixture: &Fixture,
        previews: &PreviewStore,
        token: &str,
    ) -> Result<AuthorizedWrite, ConfigProfileError> {
        authorize_apply(
            &fixture.env(),
            previews,
            &ApplyConfigProfileRequest {
                token: token.to_string(),
            },
            1_000,
        )
    }

    /// The happy path: every binding still matches, so the write the user
    /// reviewed is exactly the write that is authorized.
    #[test]
    fn preview_authority_apply_authorizes_the_reviewed_write() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), b"# keep me\nmodel = \"gpt-5\"\n");
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        let write = authorize(&fixture, &previews, &preview.token).unwrap();

        assert_eq!(write.project_id, "alpha");
        assert_eq!(write.agent, ConfigAgent::Codex);
        assert_eq!(write.target.path, fixture.codex_path());
        assert_eq!(
            write.original.as_deref(),
            Some("# keep me\nmodel = \"gpt-5\"\n")
        );
        assert!(write.document_text.contains("# keep me"));
        assert!(write.document_text.contains("gpt-5.1"));
        // Authorization alone writes nothing.
        assert_eq!(
            std::fs::read(fixture.codex_path()).unwrap(),
            b"# keep me\nmodel = \"gpt-5\"\n".to_vec()
        );
    }

    // Requirement: Apply requires an exact single-use typed preview
    // Scenario: External source change invalidates preview
    #[test]
    fn preview_authority_stale_source_refuses_the_apply() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), b"model = \"gpt-5\"\n");
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        // Fingerprint A becomes fingerprint B before the confirm arrives.
        write_source(
            &fixture.codex_path(),
            b"model = \"gpt-5\"\nweb_search = true\n",
        );

        let error = authorize(&fixture, &previews, &preview.token).unwrap_err();

        assert_eq!(error, ConfigProfileError::StalePreview);
        assert_eq!(
            std::fs::read(fixture.codex_path()).unwrap(),
            b"model = \"gpt-5\"\nweb_search = true\n".to_vec()
        );
    }

    // Requirement: Apply requires an exact single-use typed preview
    // Scenario: Profile revision change invalidates preview
    #[test]
    fn preview_authority_stale_profile_refuses_the_apply() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), b"model = \"gpt-5\"\n");
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        // The profile moves on while the dialog is open.
        set_single_codex_model(&fixture, "gpt-5.2");

        let error = authorize(&fixture, &previews, &preview.token).unwrap_err();

        assert_eq!(error, ConfigProfileError::StalePreview);
        assert_eq!(
            std::fs::read(fixture.codex_path()).unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
    }

    /// A removed assignment is not a target any more, so its outstanding token
    /// stops authorizing anything.
    #[test]
    fn preview_authority_removed_assignment_refuses_the_apply() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        remove_assignment(
            &fixture.env(),
            &RemoveConfigProfileAssignmentRequest {
                profile_id: fixture.profile_id.clone(),
                project_id: "alpha".to_string(),
                agent: ConfigAgent::Codex,
            },
        )
        .unwrap();

        assert_eq!(
            authorize(&fixture, &previews, &preview.token).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn preview_authority_expired_and_replayed_tokens_authorize_nothing() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();

        assert_eq!(
            authorize_apply(
                &fixture.env(),
                &previews,
                &ApplyConfigProfileRequest {
                    token: preview.token.clone(),
                },
                preview.expires_at + 1,
            )
            .unwrap_err(),
            ConfigProfileError::PreviewExpired
        );

        // The expired token was taken out of the store, so the replay finds
        // nothing rather than getting a second chance.
        assert_eq!(
            authorize(&fixture, &previews, &preview.token).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn preview_authority_offline_library_authorizes_nothing() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();
        let offline = ConfigProfileEnv {
            store: &fixture.store,
            library_online: false,
        };

        let error = authorize_apply(
            &offline,
            &previews,
            &ApplyConfigProfileRequest {
                token: preview.token.clone(),
            },
            1_000,
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::LibraryOffline);
        // Refused before the token was consumed, so the user can retry once the
        // Library is back.
        assert!(!previews.is_empty());
    }

    // ── DTO boundary ──

    // Requirement: Profiles persist only exact typed non-sensitive settings
    // Scenario: Secret-shaped request cannot cross the mutation boundary
    #[test]
    fn dto_boundary_requests_reject_path_scope_and_raw_document_fields() {
        // Each of these is a field the mutation boundary must not have. They
        // are grouped by the request they were aimed at.
        let create = [
            r#"{"name":"P","entries":[],"path":"/etc/passwd"}"#,
            r#"{"name":"P","entries":[],"scope":"user"}"#,
            r#"{"name":"P","entries":[],"cwd":"/tmp"}"#,
            r#"{"name":"P","entries":[],"env":{"TOKEN":"x"}}"#,
            r#"{"name":"P","entries":[],"raw":"model = \"gpt-5\""}"#,
            r#"{"name":"P","entries":[],"sourcePath":"/tmp/config.toml"}"#,
            r#"{"name":"P","entries":[],"command":"rm -rf /"}"#,
        ];
        for body in create {
            assert!(
                serde_json::from_str::<CreateConfigProfileRequest>(body).is_err(),
                "create request accepted {body}"
            );
        }

        let assignment = [
            r#"{"profileId":"p","projectId":"a","agent":"codex","targetPath":"/tmp/x"}"#,
            r#"{"profileId":"p","projectId":"a","agent":"codex","scope":"user"}"#,
            r#"{"profileId":"p","projectId":"a","agent":"codex","home":"/root"}"#,
        ];
        for body in assignment {
            assert!(
                serde_json::from_str::<SetConfigProfileAssignmentRequest>(body).is_err(),
                "assignment request accepted {body}"
            );
        }

        // Apply carries the token and nothing else.
        assert!(serde_json::from_str::<ApplyConfigProfileRequest>(
            r#"{"token":"t","documentText":"model = \"gpt-5\""}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ApplyConfigProfileRequest>(r#"{"token":"t"}"#).is_ok());
    }

    /// An entry is one Agent, one canonical key and one scalar. A nested
    /// object, an array or an extra field has nowhere to go.
    #[test]
    fn dto_boundary_entries_reject_nested_and_unknown_shapes() {
        let rejected = [
            r#"{"agent":"codex","canonicalKey":"model","value":{"type":"string","value":"gpt-5"},"path":"/tmp"}"#,
            r#"{"agent":"codex","canonicalKey":"model","value":{"type":"object","value":{"a":1}}}"#,
            r#"{"agent":"codex","canonicalKey":"model","value":{"type":"array","value":[1,2]}}"#,
            r#"{"agent":"codex","canonicalKey":"model","value":"gpt-5"}"#,
            r#"{"agent":"shell","canonicalKey":"model","value":{"type":"string","value":"gpt-5"}}"#,
        ];
        for body in rejected {
            assert!(
                serde_json::from_str::<ConfigProfileEntryDto>(body).is_err(),
                "entry accepted {body}"
            );
        }
        assert!(serde_json::from_str::<ConfigProfileEntryDto>(
            r#"{"agent":"codex","canonicalKey":"model","value":{"type":"string","value":"gpt-5"}}"#
        )
        .is_ok());
    }

    /// The whole IPC error vocabulary, as the frontend branches on it.
    #[test]
    fn dto_boundary_error_codes_are_the_contract_vocabulary() {
        let codes: Vec<&str> = [
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
        ]
        .iter()
        .map(ConfigProfileError::as_str)
        .collect();

        assert_eq!(
            codes,
            vec![
                "profile_not_found",
                "project_not_found",
                "invalid_profile_entry",
                "stale_profile",
                "profile_in_use",
                "library_offline",
                "source_invalid",
                "unsupported_symlink",
                "too_large",
                "stale_preview",
                "preview_expired",
                "write_failed",
                "atomic_replace_unsupported",
                "rollback_failed",
                "recovery_not_found",
            ]
        );
        // A code is the whole message: nothing here reads like a path, a parser
        // string or an OS error.
        assert!(codes
            .iter()
            .all(|code| !code.contains('/') && !code.contains(' ') && code.is_ascii()));
    }

    /// A source full of things this capability must never repeat back: unknown
    /// keys, credentials, commands, environment values and comments.
    const SECRET_BEARING_SOURCE: &[u8] = br#"# personal notes
model = "gpt-5"
openai_api_key = "sk-live-supersecret"

[mcp_servers.internal]
command = "/opt/private/run-agent"
env = { INTERNAL_TOKEN = "tok-abc123" }
"#;

    #[test]
    fn dto_boundary_no_response_repeats_source_content() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        write_source(&fixture.codex_path(), SECRET_BEARING_SOURCE);
        set_single_codex_model(&fixture, "gpt-5.1");

        let preview = fixture.preview(&previews, ConfigAgent::Codex).unwrap();
        let assignments = list_assignments(&fixture.env(), Some(&fixture.profile_id)).unwrap();
        let profiles = list_profiles(&fixture.env()).unwrap();

        let serialized = [
            serde_json::to_string(&preview).unwrap(),
            serde_json::to_string(&assignments).unwrap(),
            serde_json::to_string(&profiles).unwrap(),
        ]
        .join("\n");

        for forbidden in [
            "sk-live-supersecret",
            "openai_api_key",
            "tok-abc123",
            "INTERNAL_TOKEN",
            "mcp_servers",
            "/opt/private/run-agent",
            "personal notes",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "response repeated {forbidden}"
            );
        }
        // Nor does any response carry the target path or a directory of it.
        let root = fixture.alpha.display().to_string();
        assert!(!serialized.contains(&root));
        assert!(!serialized.contains(".codex"));
    }

    /// A parser failure and an OS failure both reduce to a stable code: the
    /// underlying message could carry a path or a fragment of the document.
    #[test]
    fn dto_boundary_parser_and_os_failures_reduce_to_stable_codes() {
        let fixture = preview_fixture();
        let previews = PreviewStore::new();
        set_single_codex_model(&fixture, "gpt-5.1");
        write_source(&fixture.codex_path(), b"model = \"unterminated\n");

        let parse_error = fixture.preview(&previews, ConfigAgent::Codex).unwrap_err();
        assert_eq!(parse_error.as_str(), "source_invalid");

        std::fs::remove_file(fixture.codex_path()).unwrap();
        std::fs::create_dir_all(fixture.codex_path()).unwrap();
        let os_error = fixture.preview(&previews, ConfigAgent::Codex).unwrap_err();
        assert_eq!(os_error.as_str(), "source_invalid");
    }

    // ── Preservation transform ──

    fn record(agent: ConfigAgent, key: &str, value: ConfigValueDto) -> ConfigProfileEntryRecord {
        ConfigProfileEntryRecord {
            agent,
            canonical_key: key.to_string(),
            value,
        }
    }

    /// Transforms a source directly, without a store, so the round trip is
    /// exercised as its own unit.
    fn transformed(
        agent: ConfigAgent,
        source: Option<&str>,
        entries: &[ConfigProfileEntryRecord],
    ) -> Result<ConfigTransform, ConfigProfileError> {
        let target = ConfigProfileTarget {
            source_id: project_source_id(agent),
            agent,
            format: match agent {
                ConfigAgent::Codex => ConfigFormat::Toml,
                ConfigAgent::ClaudeCode => ConfigFormat::Json,
            },
            path: std::path::PathBuf::from("/unused/in/this/test"),
            state: match source {
                None => ConfigTargetState::Missing,
                Some(_) => ConfigTargetState::Present,
            },
        };
        transform_target(&target, source, entries)
    }

    // Requirement: Agent-specific transformation preserves unselected content
    // Scenario: Codex comments and unknown tables survive apply
    #[test]
    fn preservation_transform_codex_keeps_comments_unknown_tables_and_ordering() {
        let source = "# top comment\n\
                      model = \"gpt-5\"  # trailing note\n\
                      unknown_scalar = 42\n\
                      \n\
                      [model_providers.openai]\n\
                      name = \"OpenAI\"\n\
                      \n\
                      [mcp_servers.demo]\n\
                      command = \"secret-tool\"\n";

        let result = transformed(
            ConfigAgent::Codex,
            Some(source),
            &[record(
                ConfigAgent::Codex,
                "model",
                ConfigValueDto::String("gpt-5.1".to_string()),
            )],
        )
        .unwrap();

        let after = &result.document_text;
        for preserved in [
            "# top comment",
            "# trailing note",
            "unknown_scalar = 42",
            "[model_providers.openai]",
            "name = \"OpenAI\"",
            "[mcp_servers.demo]",
            "command = \"secret-tool\"",
        ] {
            assert!(after.contains(preserved), "lost {preserved}:\n{after}");
        }
        assert!(after.contains("gpt-5.1"));
        assert!(!after.contains("\"gpt-5\""));
        // Ordering is the document's own: the tables come after the scalars in
        // the same order they were written.
        let providers = after.find("[model_providers.openai]").unwrap();
        let mcp = after.find("[mcp_servers.demo]").unwrap();
        assert!(after.find("unknown_scalar").unwrap() < providers);
        assert!(providers < mcp);

        assert_eq!(result.diff.len(), 1);
        assert_eq!(result.diff[0].status, ConfigDiffStatus::Changed);
    }

    // Requirement: Agent-specific transformation preserves unselected content
    // Scenario: Claude nested permission siblings survive apply
    #[test]
    fn preservation_transform_claude_keeps_nested_siblings_and_env() {
        let source = r#"{
  "model": "sonnet",
  "permissions": {
    "defaultMode": "default",
    "allow": ["Bash(git:*)"],
    "deny": ["Read(./.env)"]
  },
  "env": { "MY_TOKEN": "tok-123" },
  "unknownTopLevel": { "keep": true }
}"#;

        let result = transformed(
            ConfigAgent::ClaudeCode,
            Some(source),
            &[record(
                ConfigAgent::ClaudeCode,
                "permission_default_mode",
                ConfigValueDto::String("plan".to_string()),
            )],
        )
        .unwrap();

        let after: serde_json::Value = serde_json::from_str(&result.document_text).unwrap();
        assert_eq!(after["permissions"]["defaultMode"], "plan");
        assert_eq!(after["permissions"]["allow"][0], "Bash(git:*)");
        assert_eq!(after["permissions"]["deny"][0], "Read(./.env)");
        assert_eq!(after["env"]["MY_TOKEN"], "tok-123");
        assert_eq!(after["unknownTopLevel"]["keep"], true);
        // An unselected top-level key is untouched.
        assert_eq!(after["model"], "sonnet");

        assert_eq!(result.diff.len(), 1);
        assert_eq!(result.diff[0].canonical_key, "permission_default_mode");
        assert_eq!(result.diff[0].status, ConfigDiffStatus::Changed);
    }

    /// Every allowlisted key, both Agents, all three scalar types: each one
    /// changes only itself and reads back exactly as written.
    #[test]
    fn preservation_transform_every_allowlisted_key_round_trips() {
        let codex_source = "# marker\nunknown_key = \"untouched\"\n";
        let claude_source = r#"{"unknownKey": "untouched"}"#;

        let cases: Vec<(ConfigAgent, &str, ConfigValueDto)> = vec![
            (
                ConfigAgent::Codex,
                "model",
                ConfigValueDto::String("gpt-5.1".into()),
            ),
            (
                ConfigAgent::Codex,
                "model_reasoning_effort",
                ConfigValueDto::String("high".into()),
            ),
            (
                ConfigAgent::Codex,
                "model_verbosity",
                ConfigValueDto::String("low".into()),
            ),
            (
                ConfigAgent::Codex,
                "approval_policy",
                ConfigValueDto::String("on-request".into()),
            ),
            (
                ConfigAgent::Codex,
                "sandbox_mode",
                ConfigValueDto::String("read-only".into()),
            ),
            (
                ConfigAgent::Codex,
                "web_search",
                ConfigValueDto::Boolean(true),
            ),
            (
                ConfigAgent::Codex,
                "service_tier",
                ConfigValueDto::String("priority".into()),
            ),
            (
                ConfigAgent::Codex,
                "personality",
                ConfigValueDto::String("concise".into()),
            ),
            (
                ConfigAgent::ClaudeCode,
                "model",
                ConfigValueDto::String("opus".into()),
            ),
            (
                ConfigAgent::ClaudeCode,
                "always_thinking_enabled",
                ConfigValueDto::Boolean(true),
            ),
            (
                ConfigAgent::ClaudeCode,
                "auto_updates_channel",
                ConfigValueDto::String("stable".into()),
            ),
            (
                ConfigAgent::ClaudeCode,
                "cleanup_period_days",
                ConfigValueDto::Integer(20),
            ),
            (
                ConfigAgent::ClaudeCode,
                "fast_mode",
                ConfigValueDto::Boolean(false),
            ),
            (
                ConfigAgent::ClaudeCode,
                "permission_default_mode",
                ConfigValueDto::String("plan".into()),
            ),
        ];

        for (agent, key, value) in cases {
            let source = match agent {
                ConfigAgent::Codex => codex_source,
                ConfigAgent::ClaudeCode => claude_source,
            };
            let result = transformed(agent, Some(source), &[record(agent, key, value.clone())])
                .unwrap_or_else(|err| panic!("{key} failed to transform: {err:?}"));

            // The unknown content is still there.
            assert!(
                result.document_text.contains("untouched"),
                "{key} lost the unknown key"
            );
            if agent == ConfigAgent::Codex {
                assert!(
                    result.document_text.contains("# marker"),
                    "{key} lost the comment"
                );
            }
            // And the value reads back as exactly what was asked for.
            let target = ConfigProfileTarget {
                source_id: project_source_id(agent),
                agent,
                format: match agent {
                    ConfigAgent::Codex => ConfigFormat::Toml,
                    ConfigAgent::ClaudeCode => ConfigFormat::Json,
                },
                path: std::path::PathBuf::from("/unused"),
                state: ConfigTargetState::Present,
            };
            let parsed = parse_target(&target, Some(&result.document_text)).unwrap();
            let allowed = allowlisted_key(agent, key).unwrap();
            assert_eq!(
                super::current_value(&parsed, &allowed),
                Some(value),
                "{key} did not read back"
            );
        }
    }

    /// A profile that says nothing about a key is not a request to delete it.
    #[test]
    fn preservation_transform_omitted_key_is_left_in_place() {
        let source = "sandbox_mode = \"danger-full-access\"\nmodel = \"gpt-5\"\n";

        let result = transformed(
            ConfigAgent::Codex,
            Some(source),
            &[record(
                ConfigAgent::Codex,
                "model",
                ConfigValueDto::String("gpt-5.1".to_string()),
            )],
        )
        .unwrap();

        assert!(result
            .document_text
            .contains("sandbox_mode = \"danger-full-access\""));
        assert!(result
            .diff
            .iter()
            .all(|entry| entry.canonical_key != "sandbox_mode"));
    }

    /// A missing target becomes the minimal valid document that carries the
    /// profile — nothing more.
    #[test]
    fn preservation_transform_missing_target_produces_a_minimal_document() {
        let codex = transformed(
            ConfigAgent::Codex,
            None,
            &[record(
                ConfigAgent::Codex,
                "model",
                ConfigValueDto::String("gpt-5.1".to_string()),
            )],
        )
        .unwrap();
        assert_eq!(codex.document_text, "model = \"gpt-5.1\"\n");
        assert_eq!(codex.diff[0].status, ConfigDiffStatus::Added);

        let claude = transformed(
            ConfigAgent::ClaudeCode,
            None,
            &[record(
                ConfigAgent::ClaudeCode,
                "permission_default_mode",
                ConfigValueDto::String("plan".to_string()),
            )],
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&claude.document_text).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({"permissions": {"defaultMode": "plan"}})
        );
        assert_eq!(claude.diff[0].status, ConfigDiffStatus::Added);
    }

    /// Only the target Agent's entries apply: a mixed profile does not put
    /// Claude settings into a Codex file.
    #[test]
    fn preservation_transform_ignores_the_other_agents_entries() {
        let result = transformed(
            ConfigAgent::Codex,
            Some("model = \"gpt-5\"\n"),
            &[
                record(
                    ConfigAgent::Codex,
                    "model",
                    ConfigValueDto::String("gpt-5.1".to_string()),
                ),
                record(
                    ConfigAgent::ClaudeCode,
                    "cleanup_period_days",
                    ConfigValueDto::Integer(20),
                ),
            ],
        )
        .unwrap();

        assert!(!result.document_text.contains("cleanupPeriodDays"));
        assert!(!result.document_text.contains("20"));
        assert_eq!(result.diff.len(), 1);
        assert_eq!(result.diff[0].agent, ConfigAgent::Codex);
    }

    /// A key present with a shape the allowlist cannot express is overwritten
    /// with the profile's typed value rather than being left half-honoured.
    #[test]
    fn preservation_transform_wrong_shaped_key_is_replaced_not_merged() {
        let result = transformed(
            ConfigAgent::Codex,
            // Codex also accepts a table form for `approval_policy`; only the
            // string form is expressible here.
            Some("[approval_policy]\nmode = \"auto\"\n"),
            &[record(
                ConfigAgent::Codex,
                "approval_policy",
                ConfigValueDto::String("on-request".to_string()),
            )],
        )
        .unwrap();

        assert!(
            result
                .document_text
                .contains("approval_policy = \"on-request\""),
            "got:\n{}",
            result.document_text
        );
        assert_eq!(result.diff[0].status, ConfigDiffStatus::Added);
        assert_eq!(result.diff[0].before, None);
    }

    // Requirement: Agent-specific transformation preserves unselected content
    // Scenario: Transformed output fails closed
    #[test]
    fn preservation_transform_invalid_source_never_produces_a_document() {
        for (agent, source) in [
            (ConfigAgent::Codex, "model = \n"),
            (ConfigAgent::ClaudeCode, "{ \"model\": }"),
            // A JSON document that is not an object has no top-level keys.
            (ConfigAgent::ClaudeCode, "[1, 2, 3]"),
        ] {
            let error = transformed(
                agent,
                Some(source),
                &[record(
                    agent,
                    "model",
                    ConfigValueDto::String("x".to_string()),
                )],
            )
            .unwrap_err();
            assert_eq!(error, ConfigProfileError::SourceInvalid, "{source}");
        }
    }

    // ── Atomic apply and rollback ──

    /// A fixture with a recovery root, which every apply case needs.
    struct ApplyFixture {
        inner: Fixture,
        recovery: TempDir,
        previews: PreviewStore,
    }

    fn apply_fixture(source: Option<&[u8]>) -> ApplyFixture {
        let inner = preview_fixture();
        if let Some(bytes) = source {
            write_source(&inner.codex_path(), bytes);
        }
        set_single_codex_model(&inner, "gpt-5.1");
        ApplyFixture {
            inner,
            recovery: TempDir::new().unwrap(),
            previews: PreviewStore::new(),
        }
    }

    impl ApplyFixture {
        fn env(&self) -> ConfigProfileEnv<'_> {
            self.inner.env()
        }

        fn write_env(&self, fault: Option<ConfigProfileFaultPoint>) -> ConfigProfileWriteEnv<'_> {
            ConfigProfileWriteEnv {
                profile: self.env(),
                recovery_root: self.recovery.path(),
                fault,
            }
        }

        /// Previews and immediately confirms, which is the whole apply path.
        fn apply(
            &self,
            fault: Option<ConfigProfileFaultPoint>,
        ) -> Result<ConfigProfileApplyOutcome, ConfigProfileError> {
            let preview = self.inner.preview(&self.previews, ConfigAgent::Codex)?;
            apply_config_profile(
                &self.write_env(fault),
                &self.previews,
                &ApplyConfigProfileRequest {
                    token: preview.token,
                },
                1_000,
            )
        }

        fn source_bytes(&self) -> Option<Vec<u8>> {
            std::fs::read(self.inner.codex_path()).ok()
        }

        /// Every file left in the target directory, so a staged file that
        /// survived a failure is visible.
        fn target_dir_entries(&self) -> Vec<String> {
            let dir = self.inner.codex_path().parent().unwrap().to_path_buf();
            let mut names: Vec<String> = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        }

        fn assignment(&self) -> ConfigProfileAssignmentDto {
            list_assignments(&self.env(), Some(&self.inner.profile_id))
                .unwrap()
                .into_iter()
                .find(|a| a.agent == ConfigAgent::Codex)
                .unwrap()
        }
    }

    // Requirement: Apply is atomic, recoverable, and state-consistent
    // Scenario: Successful apply records recovery and deployment state
    #[test]
    fn atomic_apply_faults_success_records_recovery_and_deployment_state() {
        let fixture = apply_fixture(Some(b"# keep me\nmodel = \"gpt-5\"\n"));

        let outcome = fixture.apply(None).unwrap();

        // The target holds the transformed document, comment intact.
        let after = String::from_utf8(fixture.source_bytes().unwrap()).unwrap();
        assert!(after.contains("# keep me"));
        assert!(after.contains("gpt-5.1"));
        assert_eq!(
            outcome.fingerprint,
            fingerprint(Some(after.as_bytes())).unwrap()
        );
        assert!(!outcome.created_file);

        // The recovery point holds the prior bytes.
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .expect("a recovery point exists");
        assert_eq!(recovery.kind, crate::core::artifact::HookBackupKind::Bytes);
        let payload = fixture.recovery.path().join(&recovery.locator);
        assert_eq!(
            std::fs::read(&payload).unwrap(),
            b"# keep me\nmodel = \"gpt-5\"\n".to_vec()
        );

        // And it is owner-private.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&payload).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "recovery payload is owner-only");
        }

        // The canonical deployment carries the new fingerprint and a clean
        // status.
        let assignment = fixture.assignment();
        assert_eq!(
            assignment.last_applied_fingerprint.as_deref(),
            Some(outcome.fingerprint.as_str())
        );
        assert!(assignment.last_applied_at.is_some());
        assert_eq!(assignment.status, "clean");
        assert!(assignment.has_recovery_point);

        // No staged file was left behind.
        assert_eq!(
            fixture.target_dir_entries(),
            vec!["config.toml".to_string()]
        );
    }

    #[test]
    fn atomic_apply_faults_success_creates_a_missing_target() {
        let fixture = apply_fixture(None);

        let outcome = fixture.apply(None).unwrap();

        assert!(outcome.created_file);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5.1\"\n".to_vec()
        );
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .expect("a recovery point exists");
        // Absence is recorded as a marker, not as an empty payload: restoring
        // an empty file would leave a zero-byte config behind.
        assert_eq!(recovery.kind, crate::core::artifact::HookBackupKind::Absent);
        assert_eq!(recovery.locator, "");
    }

    // Requirement: Apply is atomic, recoverable, and state-consistent
    // Scenario: Fault before or after replacement rolls back
    #[test]
    fn atomic_apply_faults_every_fault_point_restores_the_original() {
        for fault in [
            ConfigProfileFaultPoint::RecoveryPromote,
            ConfigProfileFaultPoint::StagedTargetSync,
            ConfigProfileFaultPoint::AtomicReplace,
            ConfigProfileFaultPoint::PostWriteVerify,
            ConfigProfileFaultPoint::SqliteCommit,
        ] {
            let fixture = apply_fixture(Some(b"# keep me\nmodel = \"gpt-5\"\n"));

            let error = fixture.apply(Some(fault)).unwrap_err();

            assert_eq!(error, ConfigProfileError::WriteFailed, "{fault:?}");
            // Exact prior bytes.
            assert_eq!(
                fixture.source_bytes().unwrap(),
                b"# keep me\nmodel = \"gpt-5\"\n".to_vec(),
                "{fault:?} did not restore the source"
            );
            // No staged file survived.
            assert_eq!(
                fixture.target_dir_entries(),
                vec!["config.toml".to_string()],
                "{fault:?} left a staged file"
            );
            // No successful deployment state.
            let assignment = fixture.assignment();
            assert_eq!(assignment.last_applied_fingerprint, None, "{fault:?}");
            assert_ne!(assignment.status, "clean", "{fault:?}");
            assert!(!assignment.has_recovery_point, "{fault:?}");
        }
    }

    /// The same fault points against a target that did not exist: the absence
    /// itself is what has to come back.
    #[test]
    fn atomic_apply_faults_restore_absence_for_a_created_target() {
        for fault in [
            ConfigProfileFaultPoint::StagedTargetSync,
            ConfigProfileFaultPoint::AtomicReplace,
            ConfigProfileFaultPoint::PostWriteVerify,
            ConfigProfileFaultPoint::SqliteCommit,
        ] {
            let fixture = apply_fixture(None);

            let error = fixture.apply(Some(fault)).unwrap_err();

            assert_eq!(error, ConfigProfileError::WriteFailed, "{fault:?}");
            assert_eq!(fixture.source_bytes(), None, "{fault:?} left a file behind");
            assert!(
                fixture.target_dir_entries().is_empty(),
                "{fault:?} left {:?}",
                fixture.target_dir_entries()
            );
            assert!(!fixture.assignment().has_recovery_point, "{fault:?}");
        }
    }

    // Requirement: Apply is atomic, recoverable, and state-consistent
    // Scenario: Rollback failure remains recoverable
    #[test]
    fn atomic_apply_faults_rollback_failure_is_explicit_and_keeps_recovery() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));

        let error = fixture
            .apply(Some(ConfigProfileFaultPoint::RollbackFailure))
            .unwrap_err();

        assert_eq!(error, ConfigProfileError::RollbackFailed);
        // The owner-private recovery point is kept so the user can restore by
        // hand; nothing claims the deployment succeeded.
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .expect("recovery is retained after a failed rollback");
        assert_eq!(
            std::fs::read(fixture.recovery.path().join(&recovery.locator)).unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
        assert_ne!(fixture.assignment().status, "clean");
    }

    // Requirement: Apply is atomic, recoverable, and state-consistent
    // Scenario: Unsupported atomic replacement fails before mutation
    #[test]
    fn atomic_apply_faults_unsupported_atomic_replace_touches_nothing() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));

        let error = fixture
            .apply(Some(ConfigProfileFaultPoint::AtomicReplaceUnsupported))
            .unwrap_err();

        assert_eq!(error, ConfigProfileError::AtomicReplaceUnsupported);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
        assert_eq!(
            fixture.target_dir_entries(),
            vec!["config.toml".to_string()]
        );
        assert!(!fixture.assignment().has_recovery_point);
        // No recovery payload was written either.
        assert!(std::fs::read_dir(fixture.recovery.path())
            .unwrap()
            .next()
            .is_none());
    }

    // Requirement: Apply is atomic, recoverable, and state-consistent
    // Scenario: Offline Library blocks persistent management but not inspection
    #[test]
    fn offline_write_gate_blocks_apply_before_any_mutation() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        let preview = fixture
            .inner
            .preview(&fixture.previews, ConfigAgent::Codex)
            .unwrap();
        let offline = ConfigProfileWriteEnv {
            profile: ConfigProfileEnv {
                store: &fixture.inner.store,
                library_online: false,
            },
            recovery_root: fixture.recovery.path(),
            fault: None,
        };

        let error = apply_config_profile(
            &offline,
            &fixture.previews,
            &ApplyConfigProfileRequest {
                token: preview.token.clone(),
            },
            1_000,
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::LibraryOffline);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
        assert!(!fixture.assignment().has_recovery_point);
        // Inspection of the fixed source is unaffected.
        let target = resolve_target(&fixture.env(), "alpha", ConfigAgent::Codex).unwrap();
        assert_eq!(
            read_target(&target).unwrap().as_deref(),
            Some("model = \"gpt-5\"\n")
        );
        // The token survives, so the user can confirm once the Library is back.
        assert!(!fixture.previews.is_empty());
    }

    /// Two applies of the same profile must not interleave their backup and
    /// replacement steps.
    #[test]
    fn atomic_apply_faults_second_apply_supersedes_the_recovery_point() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();

        set_single_codex_model(&fixture.inner, "gpt-5.2");
        fixture.apply(None).unwrap();

        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .unwrap();
        // The latest recovery point holds the state just before the second
        // apply, which is the one-step undo the user expects.
        assert_eq!(
            std::fs::read(fixture.recovery.path().join(&recovery.locator)).unwrap(),
            b"model = \"gpt-5.1\"\n".to_vec()
        );
        assert_eq!(recovery.revision, 2);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5.2\"\n".to_vec()
        );
    }

    // ── Conflict-safe restore ──

    impl ApplyFixture {
        fn preview_restore_codex(&self) -> Result<ConfigProfilePreviewDto, ConfigProfileError> {
            preview_restore(
                &self.write_env(None),
                &self.previews,
                &PreviewConfigProfileRestoreRequest {
                    profile_id: self.inner.profile_id.clone(),
                    project_id: "alpha".to_string(),
                    agent: ConfigAgent::Codex,
                },
                1_000,
            )
        }

        fn restore(&self, token: &str) -> Result<ConfigProfileApplyOutcome, ConfigProfileError> {
            apply_config_profile_restore(
                &self.write_env(None),
                &self.previews,
                &ApplyConfigProfileRequest {
                    token: token.to_string(),
                },
                1_000,
            )
        }
    }

    // Requirement: Restore is previewed and conflict-safe
    // Scenario: Existing source is restored after exact preview
    #[test]
    fn restore_contract_existing_source_comes_back_exactly() {
        let fixture = apply_fixture(Some(b"# original\nmodel = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();
        assert!(String::from_utf8(fixture.source_bytes().unwrap())
            .unwrap()
            .contains("gpt-5.1"));

        let preview = fixture.preview_restore_codex().unwrap();
        assert_eq!(preview.operation, ConfigPreviewOperation::Restore);
        // The diff reads from the applied state back to the saved one.
        assert_eq!(preview.diff.len(), 1);
        assert_eq!(preview.diff[0].canonical_key, "model");
        assert_eq!(
            preview.diff[0].before,
            Some(ConfigValueDto::String("gpt-5.1".to_string()))
        );
        assert_eq!(
            preview.diff[0].after,
            Some(ConfigValueDto::String("gpt-5".to_string()))
        );

        fixture.restore(&preview.token).unwrap();

        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"# original\nmodel = \"gpt-5\"\n".to_vec()
        );
        // The state that was just replaced became the next recovery point, so
        // the undo is itself undoable.
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.recovery.path().join(&recovery.locator)).unwrap(),
            b"# original\nmodel = \"gpt-5.1\"\n".to_vec()
        );
        assert_eq!(
            fixture.assignment().last_applied_fingerprint,
            fingerprint(Some(b"# original\nmodel = \"gpt-5\"\n"))
        );
    }

    /// apply → restore → restore returns to the applied state, which is what
    /// makes the recovery point a real one-step undo rather than a dead end.
    #[test]
    fn restore_contract_round_trips_back_to_the_applied_state() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();

        let first = fixture.preview_restore_codex().unwrap();
        fixture.restore(&first.token).unwrap();
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );

        let second = fixture.preview_restore_codex().unwrap();
        fixture.restore(&second.token).unwrap();

        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5.1\"\n".to_vec()
        );
    }

    // Requirement: Restore is previewed and conflict-safe
    // Scenario: Created source is removed by absent recovery
    #[test]
    fn restore_contract_absent_recovery_removes_the_created_file() {
        let fixture = apply_fixture(None);
        fixture.apply(None).unwrap();
        assert!(fixture.inner.codex_path().exists());

        let preview = fixture.preview_restore_codex().unwrap();
        assert!(preview.would_remove_file);
        fixture.restore(&preview.token).unwrap();

        assert!(!fixture.inner.codex_path().exists());
        // The removed file became the next recovery point.
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .unwrap();
        assert_eq!(recovery.kind, crate::core::artifact::HookBackupKind::Bytes);
        assert_eq!(
            std::fs::read(fixture.recovery.path().join(&recovery.locator)).unwrap(),
            b"model = \"gpt-5.1\"\n".to_vec()
        );
    }

    // Requirement: Restore is previewed and conflict-safe
    // Scenario: Current source change invalidates restore
    #[test]
    fn restore_contract_current_source_change_refuses_the_restore() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();
        let preview = fixture.preview_restore_codex().unwrap();

        // Fingerprint C becomes fingerprint D before the confirm arrives.
        write_source(
            &fixture.inner.codex_path(),
            b"model = \"edited-elsewhere\"\n",
        );

        let error = fixture.restore(&preview.token).unwrap_err();

        assert_eq!(error, ConfigProfileError::StalePreview);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"edited-elsewhere\"\n".to_vec()
        );
        // The recovery pointer and the deployment are untouched.
        let recovery = fixture
            .inner
            .store
            .get_config_profile_recovery(&fixture.inner.profile_id, "alpha", ConfigAgent::Codex)
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.recovery.path().join(&recovery.locator)).unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
        assert_eq!(fixture.assignment().status, "clean");
    }

    // Requirement: Restore is previewed and conflict-safe
    // Scenario: Missing recovery is explicit
    #[test]
    fn restore_contract_missing_recovery_is_explicit() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));

        let error = fixture.preview_restore_codex().unwrap_err();

        assert_eq!(error, ConfigProfileError::RecoveryNotFound);
        assert_eq!(
            fixture.source_bytes().unwrap(),
            b"model = \"gpt-5\"\n".to_vec()
        );
    }

    #[test]
    #[cfg(unix)]
    fn restore_contract_symlinked_target_is_refused() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();
        let preview = fixture.preview_restore_codex().unwrap();

        // The applied file is swapped for a link to somewhere else.
        let outside = fixture.inner.beta.join("elsewhere.toml");
        write_source(&outside, b"model = \"outside\"\n");
        std::fs::remove_file(fixture.inner.codex_path()).unwrap();
        std::os::unix::fs::symlink(&outside, fixture.inner.codex_path()).unwrap();

        let error = fixture.restore(&preview.token).unwrap_err();

        assert_eq!(error, ConfigProfileError::UnsupportedSymlink);
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"model = \"outside\"\n".to_vec()
        );
    }

    /// Raw recovery bytes never cross the backend boundary: the preview carries
    /// a typed diff and nothing else.
    #[test]
    fn restore_contract_preview_carries_no_backup_bytes() {
        let fixture = apply_fixture(Some(SECRET_BEARING_SOURCE));
        fixture.apply(None).unwrap();

        let preview = fixture.preview_restore_codex().unwrap();

        let json = serde_json::to_string(&preview).unwrap();
        for forbidden in [
            "sk-live-supersecret",
            "openai_api_key",
            "tok-abc123",
            "mcp_servers",
            "personal notes",
            ".codex",
        ] {
            assert!(
                !json.contains(forbidden),
                "restore preview leaked {forbidden}"
            );
        }
    }

    #[test]
    fn restore_contract_token_is_single_use_and_expires() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();

        let expired = fixture.preview_restore_codex().unwrap();
        assert_eq!(
            apply_config_profile_restore(
                &fixture.write_env(None),
                &fixture.previews,
                &ApplyConfigProfileRequest {
                    token: expired.token.clone(),
                },
                expired.expires_at + 1,
            )
            .unwrap_err(),
            ConfigProfileError::PreviewExpired
        );

        let used = fixture.preview_restore_codex().unwrap();
        fixture.restore(&used.token).unwrap();
        assert_eq!(
            fixture.restore(&used.token).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    /// An apply token cannot be confirmed through the restore command, and the
    /// other way round: the operation is part of what the token binds.
    #[test]
    fn restore_contract_tokens_do_not_cross_operations() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();

        let restore_token = fixture.preview_restore_codex().unwrap().token;
        assert_eq!(
            apply_config_profile(
                &fixture.write_env(None),
                &fixture.previews,
                &ApplyConfigProfileRequest {
                    token: restore_token,
                },
                1_000,
            )
            .unwrap_err(),
            ConfigProfileError::StalePreview
        );

        set_single_codex_model(&fixture.inner, "gpt-5.3");
        let apply_token = fixture
            .inner
            .preview(&fixture.previews, ConfigAgent::Codex)
            .unwrap()
            .token;
        assert_eq!(
            fixture.restore(&apply_token).unwrap_err(),
            ConfigProfileError::StalePreview
        );
    }

    #[test]
    fn restore_contract_offline_library_blocks_restore() {
        let fixture = apply_fixture(Some(b"model = \"gpt-5\"\n"));
        fixture.apply(None).unwrap();
        let preview = fixture.preview_restore_codex().unwrap();
        let offline = ConfigProfileWriteEnv {
            profile: ConfigProfileEnv {
                store: &fixture.inner.store,
                library_online: false,
            },
            recovery_root: fixture.recovery.path(),
            fault: None,
        };

        let error = apply_config_profile_restore(
            &offline,
            &fixture.previews,
            &ApplyConfigProfileRequest {
                token: preview.token.clone(),
            },
            1_000,
        )
        .unwrap_err();

        assert_eq!(error, ConfigProfileError::LibraryOffline);
        assert!(String::from_utf8(fixture.source_bytes().unwrap())
            .unwrap()
            .contains("gpt-5.1"));
        assert!(!fixture.previews.is_empty());
    }

    /// The editor builds its controls from this list, so it is the reason an
    /// arbitrary key is not expressible in the UI.
    #[test]
    fn profile_crud_writable_keys_are_exactly_the_inspection_allowlist() {
        let keys = writable_keys();
        let codex: Vec<&str> = keys
            .iter()
            .filter(|key| key.agent == ConfigAgent::Codex)
            .map(|key| key.canonical_key.as_str())
            .collect();
        let claude: Vec<&str> = keys
            .iter()
            .filter(|key| key.agent == ConfigAgent::ClaudeCode)
            .map(|key| key.canonical_key.as_str())
            .collect();

        assert_eq!(
            codex,
            vec![
                "model",
                "model_reasoning_effort",
                "model_verbosity",
                "approval_policy",
                "sandbox_mode",
                "web_search",
                "service_tier",
                "personality",
            ]
        );
        assert_eq!(
            claude,
            vec![
                "model",
                "always_thinking_enabled",
                "auto_updates_channel",
                "cleanup_period_days",
                "fast_mode",
                "permission_default_mode",
            ]
        );
        assert!(keys
            .iter()
            .all(|key| allowlisted_key(key.agent, &key.canonical_key).is_some()));
    }
}
