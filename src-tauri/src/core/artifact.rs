//! Typed Artifact identity shared by every managed item.
//!
//! Phase 3 only lands the identity and deployment vocabulary — Skill remains
//! the sole subtype with a detail table. Plugin, Hook and Config Profile get
//! their own detail tables in later changes, keyed on the same Artifact id.

use anyhow::{bail, Result};

/// The subtype an Artifact identity belongs to.
///
/// Conversion is deliberately strict in both directions: an unrecognised
/// persisted value is an error, never a fallback to `Skill`. A silent fallback
/// would route a future subtype through the Skill sync and deployment paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Skill,
    Plugin,
    Hook,
    ConfigProfile,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::Skill => "skill",
            ArtifactKind::Plugin => "plugin",
            ArtifactKind::Hook => "hook",
            ArtifactKind::ConfigProfile => "config_profile",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "skill" => Ok(ArtifactKind::Skill),
            "plugin" => Ok(ArtifactKind::Plugin),
            "hook" => Ok(ArtifactKind::Hook),
            "config_profile" => Ok(ArtifactKind::ConfigProfile),
            other => bail!(
                "unknown artifact kind: {other:?} (expected skill, plugin, hook or config_profile)"
            ),
        }
    }
}

/// An Artifact identity. Subtype fields — name, description, source metadata —
/// stay in the subtype's own detail table so adding a subtype cannot widen this
/// row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub kind: ArtifactKind,
}

/// Where a deployment applies. `Global` means every workspace; `Project` names
/// one project id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactScope {
    Global,
    Project(String),
}

impl ArtifactScope {
    pub fn scope_type(&self) -> &'static str {
        match self {
            ArtifactScope::Global => "global",
            ArtifactScope::Project(_) => "project",
        }
    }

    /// The persisted `scope_id`. Global scope stores an empty string rather
    /// than NULL so the uniqueness key stays comparable across both scopes.
    pub fn scope_id(&self) -> &str {
        match self {
            ArtifactScope::Global => "",
            ArtifactScope::Project(id) => id,
        }
    }

    pub fn parse(scope_type: &str, scope_id: &str) -> Result<Self> {
        match scope_type {
            "global" => {
                if scope_id.is_empty() {
                    Ok(ArtifactScope::Global)
                } else {
                    bail!("global deployment scope must not carry a project id: {scope_id:?}")
                }
            }
            "project" => {
                if scope_id.is_empty() {
                    bail!("project deployment scope requires a project id")
                } else {
                    Ok(ArtifactScope::Project(scope_id.to_string()))
                }
            }
            other => bail!("unknown deployment scope: {other:?} (expected global or project)"),
        }
    }

    /// Reject a scope built directly in Rust that could not have come from a
    /// valid row — `Project("")` is representable but meaningless.
    pub fn validate(&self) -> Result<()> {
        Self::parse(self.scope_type(), self.scope_id()).map(|_| ())
    }
}

/// Which configuration context a managed Hook source belongs to.
///
/// A source id is only unique inside its context: `claude_code:project:settings-json`
/// names a different file in every linked Project, so identity is keyed on both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookContext {
    Global,
    Project(String),
}

impl HookContext {
    /// The persisted key. A prefix rather than two columns keeps the source id
    /// plus context uniqueness expressible as one index.
    pub fn key(&self) -> String {
        match self {
            HookContext::Global => "global".to_string(),
            HookContext::Project(id) => format!("project:{id}"),
        }
    }

    pub fn parse(key: &str) -> Result<Self> {
        if key == "global" {
            return Ok(HookContext::Global);
        }
        match key.strip_prefix("project:") {
            Some(id) if !id.is_empty() => Ok(HookContext::Project(id.to_string())),
            _ => bail!("unknown hook context key: {key:?} (expected global or project:<id>)"),
        }
    }
}

/// Whether a recovery point holds the previous bytes, or records that the
/// source did not exist before the write.
///
/// An absent source has no payload file: writing an empty one would restore a
/// zero-byte config instead of removing the file AgentDeck created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookBackupKind {
    Bytes,
    Absent,
}

impl HookBackupKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookBackupKind::Bytes => "bytes",
            HookBackupKind::Absent => "absent",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "bytes" => Ok(HookBackupKind::Bytes),
            "absent" => Ok(HookBackupKind::Absent),
            other => bail!("unknown hook backup kind: {other:?} (expected bytes or absent)"),
        }
    }
}

/// How a deployment puts the Artifact in place.
///
/// `CliManaged` covers Agents whose own CLI owns the on-disk layout, so
/// AgentDeck records the deployment without writing the target itself.
/// `ConfigProfile` covers the managed write of allowlisted settings into a
/// fixed Project source: the target is derived from the Project record and the
/// Agent on every use, so the deployment row carries no path of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentMode {
    Symlink,
    Copy,
    CliManaged,
    ConfigProfile,
}

impl DeploymentMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentMode::Symlink => "symlink",
            DeploymentMode::Copy => "copy",
            DeploymentMode::CliManaged => "cli-managed",
            DeploymentMode::ConfigProfile => "config-profile",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "symlink" => Ok(DeploymentMode::Symlink),
            "copy" => Ok(DeploymentMode::Copy),
            "cli-managed" => Ok(DeploymentMode::CliManaged),
            "config-profile" => Ok(DeploymentMode::ConfigProfile),
            other => {
                bail!(
                    "unknown deployment mode: {other:?} \
                     (expected symlink, copy, cli-managed or config-profile)"
                )
            }
        }
    }
}

/// One canonical deployment of an Artifact to one Agent in one scope.
///
/// `status` and `last_error` hold displayable state only — never tokens,
/// credentials or command environments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDeploymentRecord {
    pub id: String,
    pub artifact_id: String,
    pub scope: ArtifactScope,
    pub agent: String,
    pub enabled: bool,
    pub mode: DeploymentMode,
    pub source_path: String,
    pub target_path: String,
    pub last_synced_hash: Option<String>,
    pub last_synced_at: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
}

#[cfg(test)]
mod legacy_boundary_tests {
    use crate::core::skill_store::{SkillRecord, SkillTargetRecord};

    /// The legacy target table exists only inside the migration that retires
    /// it. A production query left behind would compile but fail at runtime
    /// against a schema v8 database, so the boundary is asserted here rather
    /// than found by a user. Test modules are exempt — migration fixtures have
    /// to name the table they migrate — so only the source above the first
    /// `#[cfg(test)]` is checked. The name is assembled at compile time so this
    /// file is not its own offender.
    #[test]
    fn no_production_source_queries_the_legacy_target_table() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let allowed = src.join("core").join("migrations.rs");
        let needle = concat!("skill", "_targets");

        let offenders: Vec<String> = walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
            .filter(|entry| entry.path() != allowed)
            .filter(|entry| {
                std::fs::read_to_string(entry.path()).is_ok_and(|text| {
                    // Migration fixtures legitimately build v7 databases; only
                    // the production half of a file is off limits.
                    let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
                    production.contains(needle)
                })
            })
            .map(|entry| entry.path().display().to_string())
            .collect();

        assert!(
            offenders.is_empty(),
            "these files still reference {needle}: {offenders:?}"
        );
    }

    /// The frontend and the CLI read these two shapes. Phase 3 moves their
    /// storage but must not add a field either of them has to start sending.
    #[test]
    fn skill_and_target_serialization_gains_no_fields() {
        let skill = SkillRecord {
            id: "skill-1".to_string(),
            name: "Demo".to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: "/tmp/library/demo".to_string(),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        };
        let mut skill_keys: Vec<String> = serde_json::to_value(&skill)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        skill_keys.sort();
        assert_eq!(
            skill_keys,
            vec![
                "central_path",
                "content_hash",
                "created_at",
                "description",
                "enabled",
                "id",
                "last_check_error",
                "last_checked_at",
                "name",
                "remote_revision",
                "source_branch",
                "source_ref",
                "source_ref_resolved",
                "source_revision",
                "source_subpath",
                "source_type",
                "status",
                "update_status",
                "updated_at",
            ]
        );

        let target = SkillTargetRecord {
            id: "target-1".to_string(),
            skill_id: "skill-1".to_string(),
            tool: "codex".to_string(),
            target_path: "/tmp/project/.agents/skills/demo".to_string(),
            mode: "symlink".to_string(),
            status: "ok".to_string(),
            synced_at: Some(1000),
            last_error: None,
            source_hash: Some("abc".to_string()),
        };
        let mut target_keys: Vec<String> = serde_json::to_value(&target)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        target_keys.sort();
        assert_eq!(
            target_keys,
            vec![
                "id",
                "last_error",
                "mode",
                "skill_id",
                "source_hash",
                "status",
                "synced_at",
                "target_path",
                "tool",
            ]
        );
    }
}

#[cfg(test)]
mod scope_and_mode_tests {
    use super::*;

    #[test]
    fn artifact_scope_persisted_shape_is_fixed() {
        assert_eq!(ArtifactScope::Global.scope_type(), "global");
        assert_eq!(ArtifactScope::Global.scope_id(), "");
        let project = ArtifactScope::Project("proj-1".to_string());
        assert_eq!(project.scope_type(), "project");
        assert_eq!(project.scope_id(), "proj-1");
    }

    #[test]
    fn artifact_scope_round_trips_through_persisted_columns() {
        for scope in [
            ArtifactScope::Global,
            ArtifactScope::Project("proj-1".to_string()),
        ] {
            let parsed = ArtifactScope::parse(scope.scope_type(), scope.scope_id()).unwrap();
            assert_eq!(parsed, scope);
        }
    }

    #[test]
    fn artifact_scope_rejects_ambiguous_records() {
        // Global carrying a project id, project missing its id, and any scope
        // outside the two known ones.
        assert!(ArtifactScope::parse("global", "proj-1").is_err());
        assert!(ArtifactScope::parse("project", "").is_err());
        assert!(ArtifactScope::parse("workspace", "proj-1").is_err());
        assert!(ArtifactScope::parse("", "").is_err());
    }

    #[test]
    fn artifact_scope_validates_directly_constructed_values() {
        assert!(ArtifactScope::Global.validate().is_ok());
        assert!(ArtifactScope::Project("proj-1".to_string())
            .validate()
            .is_ok());
        assert!(ArtifactScope::Project(String::new()).validate().is_err());
    }

    #[test]
    fn deployment_mode_persisted_values_are_fixed() {
        assert_eq!(DeploymentMode::Symlink.as_str(), "symlink");
        assert_eq!(DeploymentMode::Copy.as_str(), "copy");
        assert_eq!(DeploymentMode::CliManaged.as_str(), "cli-managed");
        assert_eq!(DeploymentMode::ConfigProfile.as_str(), "config-profile");
    }

    #[test]
    fn deployment_mode_round_trips_through_persisted_value() {
        for mode in [
            DeploymentMode::Symlink,
            DeploymentMode::Copy,
            DeploymentMode::CliManaged,
            DeploymentMode::ConfigProfile,
        ] {
            assert_eq!(DeploymentMode::parse(mode.as_str()).unwrap(), mode);
        }
    }

    #[test]
    fn deployment_mode_rejects_unknown_value() {
        for unknown in ["", "cli_managed", "hardlink", "Symlink"] {
            assert!(
                DeploymentMode::parse(unknown).is_err(),
                "{unknown} should be rejected"
            );
        }
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    #[test]
    fn artifact_kind_persisted_values_are_fixed() {
        assert_eq!(ArtifactKind::Skill.as_str(), "skill");
        assert_eq!(ArtifactKind::Plugin.as_str(), "plugin");
        assert_eq!(ArtifactKind::Hook.as_str(), "hook");
        assert_eq!(ArtifactKind::ConfigProfile.as_str(), "config_profile");
    }

    #[test]
    fn artifact_kind_round_trips_through_persisted_value() {
        for kind in [
            ArtifactKind::Skill,
            ArtifactKind::Plugin,
            ArtifactKind::Hook,
            ArtifactKind::ConfigProfile,
        ] {
            assert_eq!(ArtifactKind::parse(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn artifact_kind_rejects_unknown_value_without_skill_fallback() {
        // An unknown kind must surface as an error naming the value, never
        // degrade into `Skill` — a silent fallback would let a future subtype
        // be deployed through the Skill sync path.
        for unknown in ["", "skill_pack", "Skill", "SKILL", "config-profile"] {
            let err = ArtifactKind::parse(unknown).unwrap_err().to_string();
            assert!(
                err.contains(unknown) || unknown.is_empty(),
                "error should name the rejected value, got: {err}"
            );
        }
    }
}
