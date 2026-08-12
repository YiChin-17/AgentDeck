use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

use super::artifact::{
    ArtifactDeploymentRecord, ArtifactKind, ArtifactRecord, ArtifactScope, DeploymentMode,
};
use super::audit_log::{AuditDraft, AuditEntry, MAX_ENTRIES as AUDIT_MAX_ENTRIES};
use super::crypto;
use super::log_sanitize;

/// Settings keys whose values are encrypted at rest with AES-256-GCM.
const SENSITIVE_KEYS: &[&str] = &["proxy_url", "git_backup_remote_url"];

pub struct SkillStore {
    conn: Mutex<Connection>,
    secret_key: [u8; 32],
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_type: String,
    pub source_ref: Option<String>,
    pub source_ref_resolved: Option<String>,
    pub source_subpath: Option<String>,
    pub source_branch: Option<String>,
    pub source_revision: Option<String>,
    pub remote_revision: Option<String>,
    pub central_path: String,
    pub content_hash: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: String,
    pub update_status: String,
    pub last_checked_at: Option<i64>,
    pub last_check_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillTargetRecord {
    pub id: String,
    pub skill_id: String,
    pub tool: String,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub synced_at: Option<i64>,
    pub last_error: Option<String>,
    /// SHA-256 of the central skill source at the time of the last
    /// successful sync. Compared against the current `skills.content_hash`
    /// to skip redundant Copy-mode resyncs (issue #153). `None` for rows
    /// written before this column existed, or when the source had no hash.
    pub source_hash: Option<String>,
}

/// One row of the pending-conflict projection (merge-engine design §4).
#[derive(Debug, Clone, Serialize)]
pub struct PendingConflictRow {
    pub skill_id: String,
    pub theirs_commit: String,
    pub theirs_path: Option<String>,
    pub detected_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredSkillRecord {
    pub id: String,
    pub tool: String,
    pub found_path: String,
    pub name_guess: Option<String>,
    pub fingerprint: Option<String>,
    pub found_at: i64,
    pub imported_skill_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub workspace_type: String,
    pub linked_agent_key: Option<String>,
    pub linked_agent_name: Option<String>,
    pub disabled_path: Option<String>,
    pub sort_order: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioSkillToolToggleRecord {
    pub scenario_id: String,
    pub skill_id: String,
    pub tool: String,
    pub enabled: bool,
    pub updated_at: i64,
}

impl SkillStore {
    pub fn new(db_path: &PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        // busy_timeout makes concurrent CLI + GUI writers wait briefly instead
        // of failing immediately with SQLITE_BUSY. 5s is generous for any
        // realistic write contention here.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        super::migrations::run_migrations(&conn)?;

        // Derive key file path from the database directory.
        let key_path = db_path
            .parent()
            .map(|p| p.join(".secret.key"))
            .unwrap_or_else(|| PathBuf::from(".secret.key"));
        let secret_key = crypto::load_or_create_key(&key_path)?;

        Ok(Self {
            conn: Mutex::new(conn),
            secret_key,
        })
    }

    // ── Skills CRUD ──

    pub fn insert_skill(&self, skill: &SkillRecord) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        ensure_skill_artifact(&tx, &skill.id)?;
        tx.execute(
            "INSERT INTO skills (
                id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                created_at, updated_at, status, update_status, last_checked_at, last_check_error,
                artifact_id
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?1)",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.source_type,
                skill.source_ref,
                skill.source_ref_resolved,
                skill.source_subpath,
                skill.source_branch,
                skill.source_revision,
                skill.remote_revision,
                skill.central_path,
                skill.content_hash,
                skill.enabled,
                skill.created_at,
                skill.updated_at,
                skill.status,
                skill.update_status,
                skill.last_checked_at,
                skill.last_check_error,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_skill(&self, skill: &SkillRecord) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        ensure_skill_artifact(&tx, &skill.id)?;
        tx.execute(
            "INSERT INTO skills (
                id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                created_at, updated_at, status, update_status, last_checked_at, last_check_error,
                artifact_id
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?1)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                source_type = excluded.source_type,
                source_ref = excluded.source_ref,
                source_ref_resolved = excluded.source_ref_resolved,
                source_subpath = excluded.source_subpath,
                source_branch = excluded.source_branch,
                source_revision = excluded.source_revision,
                remote_revision = excluded.remote_revision,
                central_path = excluded.central_path,
                content_hash = excluded.content_hash,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at,
                status = excluded.status,
                update_status = excluded.update_status,
                last_checked_at = excluded.last_checked_at,
                last_check_error = excluded.last_check_error,
                artifact_id = excluded.artifact_id",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.source_type,
                skill.source_ref,
                skill.source_ref_resolved,
                skill.source_subpath,
                skill.source_branch,
                skill.source_revision,
                skill.remote_revision,
                skill.central_path,
                skill.content_hash,
                skill.enabled,
                skill.created_at,
                skill.updated_at,
                skill.status,
                skill.update_status,
                skill.last_checked_at,
                skill.last_check_error,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ── Artifacts ──

    /// Create an Artifact identity for a subtype other than Skill. Skill
    /// identities are created by `insert_skill`/`upsert_skill` inside the same
    /// transaction as the Skill detail, so they never need this.
    pub fn insert_artifact(&self, artifact: &ArtifactRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, kind) VALUES (?1, ?2)",
            params![artifact.id, artifact.kind.as_str()],
        )?;
        Ok(())
    }

    pub fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, kind FROM artifacts WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => {
                let (id, kind) = row?;
                Ok(Some(ArtifactRecord {
                    id,
                    kind: ArtifactKind::parse(&kind)?,
                }))
            }
        }
    }

    // ── Deployments ──

    pub fn upsert_deployment(&self, deployment: &ArtifactDeploymentRecord) -> Result<()> {
        deployment.scope.validate()?;
        let conn = self.conn.lock().unwrap();
        write_deployment(&conn, deployment)
    }

    pub fn get_deployments_for_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<Vec<ArtifactDeploymentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "{DEPLOYMENT_COLUMNS} FROM artifact_deployments WHERE artifact_id = ?1"
        ))?;
        let rows = stmt
            .query_map(params![artifact_id], map_deployment_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_all_deployments(&self) -> Result<Vec<ArtifactDeploymentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("{DEPLOYMENT_COLUMNS} FROM artifact_deployments"))?;
        let rows = stmt
            .query_map([], map_deployment_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_deployment(
        &self,
        artifact_id: &str,
        scope: &ArtifactScope,
        agent: &str,
    ) -> Result<()> {
        scope.validate()?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM artifact_deployments
             WHERE artifact_id = ?1 AND scope_type = ?2 AND scope_id = ?3 AND agent = ?4",
            params![artifact_id, scope.scope_type(), scope.scope_id(), agent],
        )?;
        Ok(())
    }

    pub fn get_all_skills(&self) -> Result<Vec<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                    source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                    created_at, updated_at, status, update_status, last_checked_at, last_check_error
             FROM skills ORDER BY name",
        )?;
        let rows = stmt.query_map([], map_skill_row)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_skill_by_id(&self, id: &str) -> Result<Option<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                    source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                    created_at, updated_at, status, update_status, last_checked_at, last_check_error
             FROM skills WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], map_skill_row)?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_skill_by_central_path(&self, central_path: &str) -> Result<Option<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                    source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                    created_at, updated_at, status, update_status, last_checked_at, last_check_error
             FROM skills WHERE central_path = ?1",
        )?;
        let mut rows = stmt.query_map(params![central_path], map_skill_row)?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_skill_by_source_ref(
        &self,
        source_type: &str,
        source_ref: &str,
    ) -> Result<Option<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, source_type, source_ref, source_ref_resolved, source_subpath,
                    source_branch, source_revision, remote_revision, central_path, content_hash, enabled,
                    created_at, updated_at, status, update_status, last_checked_at, last_check_error
             FROM skills
             WHERE source_type = ?1 AND source_ref = ?2",
        )?;
        let mut rows = stmt.query_map(params![source_type, source_ref], map_skill_row)?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn update_skill_source_metadata(
        &self,
        id: &str,
        source_ref_resolved: Option<&str>,
        source_subpath: Option<&str>,
        source_branch: Option<&str>,
        source_revision: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE skills
             SET source_ref_resolved = ?1, source_subpath = ?2, source_branch = ?3, source_revision = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                source_ref_resolved,
                source_subpath,
                source_branch,
                source_revision,
                now,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_skill_check_state(
        &self,
        id: &str,
        remote_revision: Option<&str>,
        update_status: &str,
        last_check_error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE skills
             SET remote_revision = ?1, update_status = ?2, last_checked_at = ?3, last_check_error = ?4
             WHERE id = ?5",
            params![remote_revision, update_status, now, last_check_error, id],
        )?;
        Ok(())
    }

    pub fn update_skill_update_status(&self, id: &str, update_status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE skills SET update_status = ?1 WHERE id = ?2",
            params![update_status, id],
        )?;
        Ok(())
    }

    pub fn update_skill_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE skills SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![enabled, now, id],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_skill_after_install(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        source_revision: Option<&str>,
        remote_revision: Option<&str>,
        content_hash: Option<&str>,
        update_status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE skills
             SET name = ?1, description = ?2, source_revision = ?3, remote_revision = ?4, content_hash = ?5,
                 updated_at = ?6, update_status = ?7, last_checked_at = ?6, last_check_error = NULL
             WHERE id = ?8",
            params![
                name,
                description,
                source_revision,
                remote_revision,
                content_hash,
                now,
                update_status,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_skill_source_ref(&self, id: &str, source_ref: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE skills SET source_ref = ?1 WHERE id = ?2",
            params![source_ref, id],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_skill_after_reinstall(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        source_type: &str,
        source_ref: Option<&str>,
        source_ref_resolved: Option<&str>,
        source_subpath: Option<&str>,
        source_branch: Option<&str>,
        source_revision: Option<&str>,
        remote_revision: Option<&str>,
        content_hash: Option<&str>,
        update_status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE skills
             SET name = ?1, description = ?2, source_type = ?3, source_ref = ?4, source_ref_resolved = ?5,
                 source_subpath = ?6, source_branch = ?7, source_revision = ?8, remote_revision = ?9,
                 content_hash = ?10, updated_at = ?11, status = 'ok', update_status = ?12, last_checked_at = ?11,
                 last_check_error = NULL
             WHERE id = ?13",
            params![
                name,
                description,
                source_type,
                source_ref,
                source_ref_resolved,
                source_subpath,
                source_branch,
                source_revision,
                remote_revision,
                content_hash,
                now,
                update_status,
                id
            ],
        )?;
        Ok(())
    }

    /// Park every skill's `central_path` on a unique placeholder before a
    /// reindex rewrites them. Path reassignments between existing skills
    /// (renames, collision reshuffles after a merge) would otherwise collide
    /// with the UNIQUE constraint mid-loop — e.g. skill A moving onto the
    /// path skill B is about to vacate.
    pub fn park_central_paths_for_reindex(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE skills SET central_path = 'sm-reindex-parked://' || id",
            [],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM skills WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Targets (Skill compatibility layer over canonical deployments) ──
    //
    // A legacy Skill target is exactly one global, enabled deployment. These
    // methods keep `SkillTargetRecord` as the shape callers see, so the Board,
    // sync engine, commands and CLI need no change.

    pub fn insert_target(&self, target: &SkillTargetRecord) -> Result<()> {
        let mode = DeploymentMode::parse(&target.mode)?;
        let conn = self.conn.lock().unwrap();
        // The canonical record carries the source path explicitly; for a Skill
        // it is always the central Library path.
        let source_path: String = conn.query_row(
            "SELECT central_path FROM skills WHERE id = ?1",
            params![target.skill_id],
            |row| row.get(0),
        )?;
        write_deployment(
            &conn,
            &ArtifactDeploymentRecord {
                id: target.id.clone(),
                artifact_id: target.skill_id.clone(),
                scope: ArtifactScope::Global,
                agent: target.tool.clone(),
                enabled: true,
                mode,
                source_path,
                target_path: target.target_path.clone(),
                last_synced_hash: target.source_hash.clone(),
                last_synced_at: target.synced_at,
                status: target.status.clone(),
                last_error: target.last_error.clone(),
            },
        )
    }

    pub fn get_targets_for_skill(&self, skill_id: &str) -> Result<Vec<SkillTargetRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "{GLOBAL_TARGET_COLUMNS} AND artifact_id = ?1"
        ))?;
        let rows = stmt
            .query_map(params![skill_id], map_target_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_all_targets(&self) -> Result<Vec<SkillTargetRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(GLOBAL_TARGET_COLUMNS)?;
        let rows = stmt
            .query_map([], map_target_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Remove the Skill's global deployment for `tool`. Project-scoped
    /// deployments are a separate record and are left alone; the enabled flag
    /// is ignored so a disabled row cannot survive as an unreachable orphan.
    pub fn delete_target(&self, skill_id: &str, tool: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM artifact_deployments
             WHERE artifact_id = ?1 AND scope_type = 'global' AND agent = ?2",
            params![skill_id, tool],
        )?;
        Ok(())
    }

    // ── Discovered Skills ──

    pub fn clear_discovered(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM discovered_skills", [])?;
        Ok(())
    }

    pub fn insert_discovered(&self, rec: &DiscoveredSkillRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO discovered_skills (id, tool, found_path, name_guess, fingerprint, found_at, imported_skill_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rec.id,
                rec.tool,
                rec.found_path,
                rec.name_guess,
                rec.fingerprint,
                rec.found_at,
                rec.imported_skill_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_discovered(&self) -> Result<Vec<DiscoveredSkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tool, found_path, name_guess, fingerprint, found_at, imported_skill_id FROM discovered_skills",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DiscoveredSkillRecord {
                id: row.get(0)?,
                tool: row.get(1)?,
                found_path: row.get(2)?,
                name_guess: row.get(3)?,
                fingerprint: row.get(4)?,
                found_at: row.get(5)?,
                imported_skill_id: row.get(6)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    // ── Cache ──

    pub fn get_cache(&self, key: &str, ttl_secs: i64) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn
            .prepare("SELECT data FROM skillssh_cache WHERE cache_key = ?1 AND fetched_at > ?2")?;
        let cutoff = now - ttl_secs;
        let mut rows = stmt.query_map(params![key, cutoff], |row| row.get::<_, String>(0))?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn set_cache(&self, key: &str, data: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO skillssh_cache (cache_key, data, fetched_at) VALUES (?1, ?2, ?3)",
            params![key, data, now],
        )?;
        Ok(())
    }

    // ── Pending conflicts (merge-engine design §4) ──
    // A rebuildable UI projection of the trailer-derived pending set; never
    // an input to merge decisions.

    pub fn replace_pending_conflicts(&self, rows: &[PendingConflictRow]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM pending_conflicts", [])?;
        for row in rows {
            tx.execute(
                "INSERT OR REPLACE INTO pending_conflicts
                 (skill_id, theirs_commit, theirs_path, detected_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![row.skill_id, row.theirs_commit, row.theirs_path, row.detected_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_pending_conflicts(&self) -> Result<Vec<PendingConflictRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT skill_id, theirs_commit, theirs_path, detected_at
             FROM pending_conflicts ORDER BY detected_at DESC, skill_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PendingConflictRow {
                    skill_id: row.get(0)?,
                    theirs_commit: row.get(1)?,
                    theirs_path: row.get(2)?,
                    detected_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // ── Settings ──

    pub fn proxy_url(&self) -> Option<String> {
        self.get_setting("proxy_url")
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        // Read the raw stored value while holding the lock, then release it
        // before any write-back so we don't re-enter the mutex.
        let raw = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
            let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
            rows.next().and_then(|r| r.ok())
        };

        let value = match raw {
            None => return Ok(None),
            Some(v) => v,
        };

        if SENSITIVE_KEYS.contains(&key) {
            if crypto::is_encrypted(&value) {
                // Happy path: already encrypted, just decrypt.
                Ok(Some(crypto::decrypt(&self.secret_key, &value)?))
            } else {
                // Backward compat: old plaintext value — upgrade it silently.
                let encrypted = crypto::encrypt(&self.secret_key, &value)?;
                let conn = self.conn.lock().unwrap();
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![key, encrypted],
                )?;
                Ok(Some(value))
            }
        } else {
            Ok(Some(value))
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let stored = if SENSITIVE_KEYS.contains(&key) {
            crypto::encrypt(&self.secret_key, value)?
        } else {
            value.to_string()
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, stored],
        )?;
        Ok(())
    }

    pub fn remap_tool_key_references(&self, old_key: &str, new_key: &str) -> Result<()> {
        if old_key == new_key {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();

        // scenario_skill_tools has a composite PK (scenario_id, skill_id, tool). If both old/new
        // rows exist for the same skill in the same scenario, keep the new-key row.
        conn.execute(
            "DELETE FROM scenario_skill_tools AS old_rows
             WHERE old_rows.tool = ?1
               AND EXISTS (
                 SELECT 1
                 FROM scenario_skill_tools AS new_rows
                 WHERE new_rows.tool = ?2
                   AND new_rows.scenario_id = old_rows.scenario_id
                   AND new_rows.skill_id = old_rows.skill_id
               )",
            params![old_key, new_key],
        )?;
        conn.execute(
            "UPDATE scenario_skill_tools SET tool = ?2 WHERE tool = ?1",
            params![old_key, new_key],
        )?;

        // artifact_deployments has UNIQUE(artifact_id, scope_type, scope_id, agent).
        // Same strategy: keep existing new-key rows.
        conn.execute(
            "DELETE FROM artifact_deployments AS old_rows
             WHERE old_rows.agent = ?1
               AND EXISTS (
                 SELECT 1
                 FROM artifact_deployments AS new_rows
                 WHERE new_rows.agent = ?2
                   AND new_rows.artifact_id = old_rows.artifact_id
                   AND new_rows.scope_type = old_rows.scope_type
                   AND new_rows.scope_id = old_rows.scope_id
               )",
            params![old_key, new_key],
        )?;
        conn.execute(
            "UPDATE artifact_deployments SET agent = ?2 WHERE agent = ?1",
            params![old_key, new_key],
        )?;

        conn.execute(
            "UPDATE discovered_skills SET tool = ?2 WHERE tool = ?1",
            params![old_key, new_key],
        )?;
        Ok(())
    }

    pub fn has_tool_key_references(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT EXISTS(SELECT 1 FROM artifact_deployments WHERE agent = ?1)
             OR EXISTS(SELECT 1 FROM discovered_skills WHERE tool = ?1)
             OR EXISTS(SELECT 1 FROM scenario_skill_tools WHERE tool = ?1)",
        )?;
        let exists: i64 = stmt.query_row(params![key], |row| row.get(0))?;
        Ok(exists != 0)
    }

    // ── Scenarios ──

    pub fn insert_scenario(&self, scenario: &ScenarioRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO scenarios (id, name, description, icon, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                scenario.id,
                scenario.name,
                scenario.description,
                scenario.icon,
                scenario.sort_order,
                scenario.created_at,
                scenario.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_scenarios(&self) -> Result<Vec<ScenarioRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, sort_order, created_at, updated_at FROM scenarios ORDER BY sort_order, created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ScenarioRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
                sort_order: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn update_scenario(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE scenarios SET name = ?1, description = ?2, icon = ?3, updated_at = ?4 WHERE id = ?5",
            params![name, description, icon, now, id],
        )?;
        Ok(())
    }

    pub fn delete_scenario(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM scenarios WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reorder_scenarios(&self, ids: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE scenarios SET sort_order = ?1 WHERE id = ?2",
                params![i as i32, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn reorder_projects(&self, ids: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE projects SET sort_order = ?1 WHERE id = ?2",
                params![i as i32, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ── Scenario-Skill mapping ──

    pub fn add_skill_to_scenario(&self, scenario_id: &str, skill_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR IGNORE INTO scenario_skills (scenario_id, skill_id, added_at) VALUES (?1, ?2, ?3)",
            params![scenario_id, skill_id, now],
        )?;
        Ok(())
    }

    pub fn remove_skill_from_scenario(&self, scenario_id: &str, skill_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM scenario_skills WHERE scenario_id = ?1 AND skill_id = ?2",
            params![scenario_id, skill_id],
        )?;
        Ok(())
    }

    pub fn reorder_scenario_skills(&self, scenario_id: &str, skill_ids: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for (i, skill_id) in skill_ids.iter().enumerate() {
            tx.execute(
                "UPDATE scenario_skills SET sort_order = ?1 WHERE scenario_id = ?2 AND skill_id = ?3",
                params![i as i32, scenario_id, skill_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_skill_ids_for_scenario(&self, scenario_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT skill_id FROM scenario_skills WHERE scenario_id = ?1 ORDER BY sort_order, added_at",
        )?;
        let rows = stmt.query_map(params![scenario_id], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_skills_for_scenario(&self, scenario_id: &str) -> Result<Vec<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, s.description, s.source_type, s.source_ref, s.source_ref_resolved, s.source_subpath,
                    s.source_branch, s.source_revision, s.remote_revision, s.central_path, s.content_hash, s.enabled,
                    s.created_at, s.updated_at, s.status, s.update_status, s.last_checked_at, s.last_check_error
             FROM skills s
             INNER JOIN scenario_skills ss ON s.id = ss.skill_id
             WHERE ss.scenario_id = ?1
             ORDER BY ss.sort_order, s.name",
        )?;
        let rows = stmt.query_map(params![scenario_id], map_skill_row)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn count_skills_for_scenario(&self, scenario_id: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scenario_skills WHERE scenario_id = ?1",
            params![scenario_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_scenarios_for_skill(&self, skill_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT scenario_id FROM scenario_skills WHERE skill_id = ?1")?;
        let rows = stmt.query_map(params![skill_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn ensure_scenario_skill_tool_defaults(
        &self,
        scenario_id: &str,
        skill_id: &str,
        tools: &[String],
    ) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }

        let conn = self.conn.lock().unwrap();
        let mut existing_stmt = conn.prepare(
            "SELECT tool
             FROM scenario_skill_tools
             WHERE scenario_id = ?1 AND skill_id = ?2",
        )?;
        let existing_rows = existing_stmt.query_map(params![scenario_id, skill_id], |row| {
            row.get::<_, String>(0)
        })?;
        let existing_tools: std::collections::HashSet<String> = existing_rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .collect();

        let missing_tools: Vec<&String> = tools
            .iter()
            .filter(|tool| !existing_tools.contains(*tool))
            .collect();
        if missing_tools.is_empty() {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        let now = chrono::Utc::now().timestamp_millis();

        for tool in missing_tools {
            tx.execute(
                "INSERT OR IGNORE INTO scenario_skill_tools (scenario_id, skill_id, tool, enabled, updated_at)
                 VALUES (?1, ?2, ?3, 1, ?4)",
                params![scenario_id, skill_id, tool, now],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn set_scenario_skill_tool_enabled(
        &self,
        scenario_id: &str,
        skill_id: &str,
        tool: &str,
        enabled: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO scenario_skill_tools (scenario_id, skill_id, tool, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(scenario_id, skill_id, tool)
             DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
            params![scenario_id, skill_id, tool, enabled, now],
        )?;
        Ok(())
    }

    pub fn replace_scenarios_from_metadata(
        &self,
        scenarios: &[super::sync_metadata::ScenarioMetaFile],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let metadata_ids: std::collections::HashSet<&str> =
            scenarios.iter().map(|s| s.scenario_id.as_str()).collect();
        {
            let mut stmt = tx.prepare("SELECT id FROM scenarios")?;
            let ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for id in ids {
                if !metadata_ids.contains(id.as_str()) {
                    tx.execute("DELETE FROM scenarios WHERE id = ?1", params![id])?;
                }
            }
        }
        let now = chrono::Utc::now().timestamp_millis();
        for scenario in scenarios {
            tx.execute(
                "INSERT INTO scenarios (id, name, description, icon, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    description = excluded.description,
                    icon = excluded.icon,
                    sort_order = excluded.sort_order,
                    updated_at = excluded.updated_at",
                params![
                    scenario.scenario_id,
                    scenario.name,
                    scenario.description,
                    scenario.icon,
                    scenario.sort_order,
                    now,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_scenario_memberships_from_metadata(
        &self,
        memberships: &[super::sync_metadata::ScenarioSkillMetaFile],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM scenario_skill_tools", [])?;
        tx.execute("DELETE FROM scenario_skills", [])?;

        // OR IGNORE / OR REPLACE don't suppress FK violations in SQLite, so we
        // must skip memberships that reference skills or scenarios no longer in the DB.
        let valid_skill_ids: std::collections::HashSet<String> = {
            let mut stmt = tx.prepare("SELECT id FROM skills")?;
            let ids: rusqlite::Result<std::collections::HashSet<String>> =
                stmt.query_map([], |row| row.get::<_, String>(0))?.collect();
            ids?
        };
        let valid_scenario_ids: std::collections::HashSet<String> = {
            let mut stmt = tx.prepare("SELECT id FROM scenarios")?;
            let ids: rusqlite::Result<std::collections::HashSet<String>> =
                stmt.query_map([], |row| row.get::<_, String>(0))?.collect();
            ids?
        };

        let now = chrono::Utc::now().timestamp_millis();
        for member in memberships {
            if !valid_skill_ids.contains(&member.skill_id)
                || !valid_scenario_ids.contains(&member.scenario_id)
            {
                log::warn!(
                    "Skipping stale scenario membership (scenario_id={}, skill_id={}): referenced skill or scenario no longer exists",
                    member.scenario_id,
                    member.skill_id
                );
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO scenario_skills (scenario_id, skill_id, added_at, sort_order)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    member.scenario_id,
                    member.skill_id,
                    now,
                    member.sort_order,
                ],
            )?;
            for (tool, enabled) in &member.tools {
                tx.execute(
                    "INSERT OR REPLACE INTO scenario_skill_tools (scenario_id, skill_id, tool, enabled, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        member.scenario_id,
                        member.skill_id,
                        tool,
                        enabled,
                        now,
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_scenario_skill_tool_toggles(
        &self,
        scenario_id: &str,
        skill_id: &str,
    ) -> Result<Vec<ScenarioSkillToolToggleRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT scenario_id, skill_id, tool, enabled, updated_at
             FROM scenario_skill_tools
             WHERE scenario_id = ?1 AND skill_id = ?2
             ORDER BY tool",
        )?;
        let rows = stmt.query_map(params![scenario_id, skill_id], |row| {
            Ok(ScenarioSkillToolToggleRecord {
                scenario_id: row.get(0)?,
                skill_id: row.get(1)?,
                tool: row.get(2)?,
                enabled: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_enabled_tools_for_scenario_skill(
        &self,
        scenario_id: &str,
        skill_id: &str,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT tool
             FROM scenario_skill_tools
             WHERE scenario_id = ?1 AND skill_id = ?2 AND enabled = 1",
        )?;
        let rows = stmt.query_map(params![scenario_id, skill_id], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ── Active Scenario ──

    pub fn get_active_scenario_id(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT scenario_id FROM active_scenario WHERE key = 'current'")?;
        let mut rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        Ok(rows.next().and_then(|r| r.ok()).flatten())
    }

    pub fn clear_active_scenario(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM active_scenario WHERE key = 'current'", [])?;
        Ok(())
    }

    pub fn set_active_scenario(&self, scenario_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO active_scenario (key, scenario_id) VALUES ('current', ?1)",
            params![scenario_id],
        )?;
        Ok(())
    }

    // ── Projects ──

    pub fn insert_project(&self, project: &ProjectRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO projects (
                id, name, path, workspace_type, linked_agent_key, linked_agent_name, disabled_path,
                sort_order, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                project.id,
                project.name,
                project.path,
                project.workspace_type,
                project.linked_agent_key,
                project.linked_agent_name,
                project.disabled_path,
                project.sort_order,
                project.created_at,
                project.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_projects(&self) -> Result<Vec<ProjectRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, path, workspace_type, linked_agent_key, linked_agent_name, disabled_path,
                    sort_order, created_at, updated_at
             FROM projects
             ORDER BY sort_order, created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                workspace_type: row.get(3)?,
                linked_agent_key: row.get(4)?,
                linked_agent_name: row.get(5)?,
                disabled_path: row.get(6)?,
                sort_order: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_project_by_id(&self, id: &str) -> Result<Option<ProjectRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, path, workspace_type, linked_agent_key, linked_agent_name, disabled_path,
                    sort_order, created_at, updated_at
             FROM projects
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                workspace_type: row.get(3)?,
                linked_agent_key: row.get(4)?,
                linked_agent_name: row.get(5)?,
                disabled_path: row.get(6)?,
                sort_order: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Skill Tags ──

    pub fn get_all_tags(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT tag FROM skill_tags ORDER BY tag")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn set_tags_for_skill(&self, skill_id: &str, tags: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM skill_tags WHERE skill_id = ?1",
            params![skill_id],
        )?;
        for tag in tags {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                conn.execute(
                    "INSERT OR IGNORE INTO skill_tags (skill_id, tag) VALUES (?1, ?2)",
                    params![skill_id, trimmed],
                )?;
            }
        }
        Ok(())
    }

    pub fn get_tags_map(&self) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT skill_id, tag FROM skill_tags ORDER BY tag")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in rows.filter_map(|r| r.ok()) {
            map.entry(row.0).or_default().push(row.1);
        }
        Ok(map)
    }

    /// Globally rename a tag across every skill that carries it. Returns the
    /// ids of the affected skills so the caller can refresh their metadata.
    /// If a skill already has `new`, the rows are merged (no duplicate) thanks
    /// to `UPDATE OR IGNORE` followed by removing any leftover old rows.
    pub fn rename_tag(&self, old: &str, new: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let affected: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT DISTINCT skill_id FROM skill_tags WHERE tag = ?1")?;
            let rows = stmt.query_map(params![old], |row| row.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        // Guard self-rename: the cleanup DELETE below would otherwise wipe the
        // tag entirely (the UPDATE is a no-op when old == new).
        if old == new {
            return Ok(affected);
        }
        // One transaction so a crash can't leave the tag half-renamed (the
        // non-conflicting rows updated while merged rows still hold `old`).
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE OR IGNORE skill_tags SET tag = ?1 WHERE tag = ?2",
            params![new, old],
        )?;
        tx.execute("DELETE FROM skill_tags WHERE tag = ?1", params![old])?;
        tx.commit()?;
        Ok(affected)
    }

    /// Globally remove a tag from every skill that carries it. Returns the ids
    /// of the affected skills so the caller can refresh their metadata.
    pub fn delete_tag(&self, name: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let affected: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT DISTINCT skill_id FROM skill_tags WHERE tag = ?1")?;
            let rows = stmt.query_map(params![name], |row| row.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        conn.execute("DELETE FROM skill_tags WHERE tag = ?1", params![name])?;
        Ok(affected)
    }

    // ── Audit log ──

    /// Append an audit entry. Best-effort: errors are swallowed so callers
    /// never have to wrap or propagate them. Auto-prunes when the table
    /// grows beyond AUDIT_MAX_ENTRIES.
    pub fn log_audit(&self, draft: AuditDraft) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        let insert = conn.execute(
            "INSERT INTO audit_log (ts, action, skill_id, skill_name, tool, success, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ts,
                draft.action,
                draft.skill_id,
                draft.skill_name,
                draft.tool,
                draft.success as i32,
                draft.detail,
            ],
        );
        if insert.is_err() {
            return;
        }
        // Prune to MAX_ENTRIES newest. Cheap when under the cap (DELETE matches 0 rows).
        let _ = conn.execute(
            "DELETE FROM audit_log WHERE id IN (
                 SELECT id FROM audit_log ORDER BY id DESC LIMIT -1 OFFSET ?1
             )",
            params![AUDIT_MAX_ENTRIES],
        );
    }

    /// Read the most recent audit entries (newest first). When `limit` is
    /// `None`, returns everything.
    pub fn list_audit(&self, limit: Option<i64>) -> Result<Vec<AuditEntry>> {
        let conn = self.conn.lock().unwrap();
        let sql = if limit.is_some() {
            "SELECT id, ts, action, skill_id, skill_name, tool, success, detail
             FROM audit_log ORDER BY id DESC LIMIT ?1"
        } else {
            "SELECT id, ts, action, skill_id, skill_name, tool, success, detail
             FROM audit_log ORDER BY id DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<AuditEntry> {
            Ok(AuditEntry {
                id: row.get(0)?,
                ts: row.get(1)?,
                action: row.get(2)?,
                skill_id: row.get(3)?,
                skill_name: row.get(4)?,
                tool: row.get(5)?,
                success: row.get::<_, i32>(6)? != 0,
                detail: row.get(7)?,
            })
        };
        let rows = if let Some(n) = limit {
            stmt.query_map(params![n], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map([], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }
}

#[cfg(test)]
mod audit_log_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn log_audit_appends_and_lists_newest_first() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();

        store.log_audit(AuditDraft::new("install").skill("id1", "first").ok());
        store.log_audit(AuditDraft::new("install").skill("id2", "second").ok());
        store.log_audit(
            AuditDraft::new("remove")
                .skill("id1", "first")
                .fail("missing"),
        );

        let entries = store.list_audit(None).unwrap();
        assert_eq!(entries.len(), 3);
        // Newest first
        assert_eq!(entries[0].action, "remove");
        assert!(!entries[0].success);
        assert_eq!(entries[0].detail.as_deref(), Some("missing"));
        assert_eq!(entries[2].action, "install");
        assert_eq!(entries[2].skill_name.as_deref(), Some("first"));
    }

    #[test]
    fn log_audit_respects_limit() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        for i in 0..5 {
            store.log_audit(AuditDraft::new("sync").detail(format!("{i}")).ok());
        }
        let entries = store.list_audit(Some(2)).unwrap();
        assert_eq!(entries.len(), 2);
        // Newest first — latest detail is "4".
        assert_eq!(entries[0].detail.as_deref(), Some("4"));
    }
}

#[cfg(test)]
mod scenario_membership_tests {
    use super::*;
    use crate::core::sync_metadata::ScenarioSkillMetaFile;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn sample_skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: format!("/tmp/{id}"),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        }
    }

    fn membership(scenario_id: &str, skill_id: &str) -> ScenarioSkillMetaFile {
        let mut tools = BTreeMap::new();
        tools.insert("ToolA".to_string(), true);
        ScenarioSkillMetaFile {
            schema_version: 1,
            scenario_id: scenario_id.to_string(),
            skill_id: skill_id.to_string(),
            sort_order: 0,
            tools,
        }
    }

    #[test]
    fn skips_memberships_referencing_missing_skill_or_scenario() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();

        store.insert_scenario(&ScenarioRecord {
            id: "s1".to_string(),
            name: "S1".to_string(),
            description: None,
            icon: None,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        })
        .unwrap();
        store.upsert_skill(&sample_skill("k1")).unwrap();

        let memberships = vec![
            membership("s1", "k1"),       // valid
            membership("s1", "ghost"),    // skill missing
            membership("ghost-s", "k1"),  // scenario missing
        ];

        // Must not panic with a FOREIGN KEY constraint failure.
        store
            .replace_scenario_memberships_from_metadata(&memberships)
            .unwrap();

        assert_eq!(store.get_skill_ids_for_scenario("s1").unwrap(), vec!["k1"]);
        assert_eq!(
            store.get_enabled_tools_for_scenario_skill("s1", "k1").unwrap(),
            vec!["ToolA"]
        );
        assert!(store
            .get_enabled_tools_for_scenario_skill("ghost-s", "k1")
            .unwrap()
            .is_empty());
    }
}

#[cfg(test)]
mod artifact_identity_tests {
    use super::*;
    use crate::core::artifact::{
        ArtifactDeploymentRecord, ArtifactKind, ArtifactRecord, ArtifactScope, DeploymentMode,
    };
    use tempfile::tempdir;

    fn skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: format!("/tmp/{id}"),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        }
    }

    fn deployment(id: &str, artifact_id: &str, agent: &str) -> ArtifactDeploymentRecord {
        ArtifactDeploymentRecord {
            id: id.to_string(),
            artifact_id: artifact_id.to_string(),
            scope: ArtifactScope::Global,
            agent: agent.to_string(),
            enabled: true,
            mode: DeploymentMode::Symlink,
            source_path: format!("/tmp/{artifact_id}"),
            target_path: format!("/tmp/target/{artifact_id}-{agent}"),
            last_synced_hash: None,
            last_synced_at: None,
            status: "ok".to_string(),
            last_error: None,
        }
    }

    #[test]
    fn insert_skill_creates_kind_skill_artifact() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();

        let artifact = store.get_artifact("a").unwrap().expect("artifact exists");
        assert_eq!(artifact.id, "a");
        assert_eq!(artifact.kind, ArtifactKind::Skill);
    }

    #[test]
    fn upsert_skill_creates_kind_skill_artifact() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.upsert_skill(&skill("a")).unwrap();
        // A second upsert must not duplicate or re-type the identity.
        store.upsert_skill(&skill("a")).unwrap();

        let artifact = store.get_artifact("a").unwrap().expect("artifact exists");
        assert_eq!(artifact.kind, ArtifactKind::Skill);
    }

    #[test]
    fn failed_skill_insert_leaves_no_partial_artifact() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();

        // `central_path` is UNIQUE, so the detail write fails after the parent
        // identity would have been written.
        let mut clash = skill("b");
        clash.central_path = "/tmp/a".to_string();
        assert!(store.insert_skill(&clash).is_err());

        assert!(store.get_artifact("b").unwrap().is_none());
        assert!(store.get_skill_by_id("b").unwrap().is_none());
        assert_eq!(store.get_all_skills().unwrap().len(), 1);
    }

    #[test]
    fn skill_detail_cannot_attach_to_non_skill_artifact() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store
            .insert_artifact(&ArtifactRecord {
                id: "p1".to_string(),
                kind: ArtifactKind::Plugin,
            })
            .unwrap();
        store.insert_skill(&skill("keep")).unwrap();

        assert!(store.insert_skill(&skill("p1")).is_err());

        // The plugin identity keeps its kind and no Skill detail was created.
        assert_eq!(
            store.get_artifact("p1").unwrap().unwrap().kind,
            ArtifactKind::Plugin
        );
        assert!(store.get_skill_by_id("p1").unwrap().is_none());
        assert_eq!(store.get_all_skills().unwrap().len(), 1);
    }

    #[test]
    fn delete_skill_removes_only_its_own_identity_and_deployments() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();
        store.insert_skill(&skill("b")).unwrap();
        store
            .insert_scenario(&ScenarioRecord {
                id: "s1".to_string(),
                name: "S1".to_string(),
                description: None,
                icon: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        for id in ["a", "b"] {
            store.set_tags_for_skill(id, &["t".into()]).unwrap();
            store.add_skill_to_scenario("s1", id).unwrap();
            store
                .upsert_deployment(&deployment(&format!("d-{id}"), id, "codex"))
                .unwrap();
        }

        store.delete_skill("a").unwrap();

        assert!(store.get_artifact("a").unwrap().is_none());
        assert!(store.get_deployments_for_artifact("a").unwrap().is_empty());
        assert!(store.get_tags_map().unwrap().get("a").is_none());
        assert_eq!(store.get_skill_ids_for_scenario("s1").unwrap(), vec!["b"]);

        // The unrelated Skill keeps identity, deployment, tags and membership.
        assert_eq!(
            store.get_artifact("b").unwrap().unwrap().kind,
            ArtifactKind::Skill
        );
        assert_eq!(store.get_deployments_for_artifact("b").unwrap().len(), 1);
        assert_eq!(store.get_tags_map().unwrap().get("b").unwrap(), &vec!["t".to_string()]);
    }
}

#[cfg(test)]
mod deployment_tests {
    use super::*;
    use crate::core::artifact::{ArtifactDeploymentRecord, ArtifactScope, DeploymentMode};
    use tempfile::tempdir;

    fn skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: format!("/tmp/library/{id}"),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        }
    }

    fn deployment(
        id: &str,
        artifact_id: &str,
        scope: ArtifactScope,
        agent: &str,
        mode: DeploymentMode,
    ) -> ArtifactDeploymentRecord {
        ArtifactDeploymentRecord {
            id: id.to_string(),
            artifact_id: artifact_id.to_string(),
            scope,
            agent: agent.to_string(),
            enabled: true,
            mode,
            source_path: format!("/tmp/library/{artifact_id}"),
            target_path: format!("/tmp/target/{id}"),
            last_synced_hash: Some("abc".to_string()),
            last_synced_at: Some(1000),
            status: "ok".to_string(),
            last_error: None,
        }
    }

    fn store_with_skill(dir: &std::path::Path, id: &str) -> SkillStore {
        let store = SkillStore::new(&dir.join("test.db")).unwrap();
        store.insert_skill(&skill(id)).unwrap();
        store
    }

    #[test]
    fn deployment_round_trips_global_and_project_scope() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        let global = deployment(
            "d-global",
            "skill-1",
            ArtifactScope::Global,
            "codex",
            DeploymentMode::Symlink,
        );
        let project = deployment(
            "d-project",
            "skill-1",
            ArtifactScope::Project("proj-1".to_string()),
            "codex",
            DeploymentMode::Symlink,
        );
        store.upsert_deployment(&global).unwrap();
        store.upsert_deployment(&project).unwrap();

        let mut rows = store.get_deployments_for_artifact("skill-1").unwrap();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].scope, ArtifactScope::Global);
        assert_eq!(
            rows[1].scope,
            ArtifactScope::Project("proj-1".to_string())
        );
        // Every column survives the round-trip unchanged.
        assert_eq!(rows[0].target_path, global.target_path);
        assert_eq!(rows[0].source_path, global.source_path);
        assert_eq!(rows[0].last_synced_hash, global.last_synced_hash);
        assert_eq!(rows[0].last_synced_at, global.last_synced_at);
        assert_eq!(rows[0].status, global.status);
        assert!(rows[0].enabled);
    }

    #[test]
    fn deployment_round_trips_all_supported_modes() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        for (agent, mode) in [
            ("codex", DeploymentMode::Symlink),
            ("claude", DeploymentMode::Copy),
            ("gemini", DeploymentMode::CliManaged),
        ] {
            store
                .upsert_deployment(&deployment(
                    &format!("d-{agent}"),
                    "skill-1",
                    ArtifactScope::Global,
                    agent,
                    mode,
                ))
                .unwrap();
        }

        let rows = store.get_deployments_for_artifact("skill-1").unwrap();
        let mut modes: Vec<String> = rows.iter().map(|r| r.mode.as_str().to_string()).collect();
        modes.sort();
        assert_eq!(modes, vec!["cli-managed", "copy", "symlink"]);
    }

    #[test]
    fn deployment_uniqueness_key_is_artifact_scope_agent() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        store
            .upsert_deployment(&deployment(
                "d-1",
                "skill-1",
                ArtifactScope::Global,
                "codex",
                DeploymentMode::Symlink,
            ))
            .unwrap();
        // Same (artifact, scope, agent) replaces rather than duplicates.
        let mut replacement = deployment(
            "d-2",
            "skill-1",
            ArtifactScope::Global,
            "codex",
            DeploymentMode::Copy,
        );
        replacement.target_path = "/tmp/target/replaced".to_string();
        store.upsert_deployment(&replacement).unwrap();

        let rows = store.get_deployments_for_artifact("skill-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target_path, "/tmp/target/replaced");
        assert_eq!(rows[0].mode, DeploymentMode::Copy);

        // A different agent or a different scope is a different deployment.
        store
            .upsert_deployment(&deployment(
                "d-3",
                "skill-1",
                ArtifactScope::Global,
                "claude",
                DeploymentMode::Symlink,
            ))
            .unwrap();
        store
            .upsert_deployment(&deployment(
                "d-4",
                "skill-1",
                ArtifactScope::Project("proj-1".to_string()),
                "codex",
                DeploymentMode::Symlink,
            ))
            .unwrap();
        assert_eq!(store.get_deployments_for_artifact("skill-1").unwrap().len(), 3);
    }

    #[test]
    fn deployment_rejects_invalid_scope_without_writing() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        let invalid = deployment(
            "d-bad",
            "skill-1",
            ArtifactScope::Project(String::new()),
            "codex",
            DeploymentMode::Symlink,
        );
        assert!(store.upsert_deployment(&invalid).is_err());
        assert!(store.get_deployments_for_artifact("skill-1").unwrap().is_empty());
    }

    #[test]
    fn deployment_rejects_orphan_artifact_reference() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        let orphan = deployment(
            "d-orphan",
            "missing",
            ArtifactScope::Global,
            "codex",
            DeploymentMode::Symlink,
        );
        assert!(store.upsert_deployment(&orphan).is_err());
        assert!(store.get_all_deployments().unwrap().is_empty());
    }

    #[test]
    fn deployment_table_rejects_unknown_scope_and_mode_via_direct_sql() {
        let tmp = tempdir().unwrap();
        let db = tmp.path().join("test.db");
        let store = store_with_skill(tmp.path(), "skill-1");
        drop(store);

        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let insert = "INSERT INTO artifact_deployments
             (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path, target_path,
              last_synced_hash, last_synced_at, status, last_error)
             VALUES (?1, 'skill-1', ?2, ?3, 'codex', ?4, ?5, '/tmp/s', '/tmp/t', NULL, NULL, 'ok', NULL)";

        // Unknown scope, project scope without an id, global scope carrying one,
        // unknown mode and a non-boolean enabled must all be refused by the
        // schema itself, not only by the Rust API.
        assert!(conn
            .execute(insert, params!["x1", "workspace", "proj-1", 1, "symlink"])
            .is_err());
        assert!(conn
            .execute(insert, params!["x2", "project", "", 1, "symlink"])
            .is_err());
        assert!(conn
            .execute(insert, params!["x3", "global", "proj-1", 1, "symlink"])
            .is_err());
        assert!(conn
            .execute(insert, params!["x4", "global", "", 1, "hardlink"])
            .is_err());
        assert!(conn
            .execute(insert, params!["x5", "global", "", 2, "symlink"])
            .is_err());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM artifact_deployments", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn artifact_tables_hold_no_secret_columns() {
        let tmp = tempdir().unwrap();
        let db = tmp.path().join("test.db");
        let store = SkillStore::new(&db).unwrap();
        drop(store);

        let conn = Connection::open(&db).unwrap();
        for table in ["artifacts", "artifact_deployments"] {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let columns: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert!(!columns.is_empty(), "{table} must exist");
            for column in &columns {
                let lower = column.to_ascii_lowercase();
                for banned in ["token", "secret", "credential", "password", "env", "login"] {
                    assert!(
                        !lower.contains(banned),
                        "{table}.{column} looks like secret storage"
                    );
                }
            }
        }
    }

    #[test]
    fn deployment_error_text_is_sanitized_before_storage() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        let mut record = deployment(
            "d-err",
            "skill-1",
            ArtifactScope::Global,
            "codex",
            DeploymentMode::Symlink,
        );
        record.status = "error".to_string();
        record.last_error = Some("push failed for ghp_ABCDEFGHIJKLMNOPQRST".to_string());
        store.upsert_deployment(&record).unwrap();

        let stored = store.get_deployments_for_artifact("skill-1").unwrap();
        let last_error = stored[0].last_error.clone().unwrap();
        assert!(!last_error.contains("ghp_ABCDEFGHIJKLMNOPQRST"));
        assert!(last_error.contains("<token>"));
    }

    #[test]
    fn legacy_target_api_reads_only_global_enabled_deployments() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        let global_enabled = deployment(
            "target-1",
            "skill-1",
            ArtifactScope::Global,
            "codex",
            DeploymentMode::Symlink,
        );
        store.upsert_deployment(&global_enabled).unwrap();

        let mut global_disabled = deployment(
            "d-disabled",
            "skill-1",
            ArtifactScope::Global,
            "claude",
            DeploymentMode::Copy,
        );
        global_disabled.enabled = false;
        store.upsert_deployment(&global_disabled).unwrap();

        store
            .upsert_deployment(&deployment(
                "d-project",
                "skill-1",
                ArtifactScope::Project("proj-1".to_string()),
                "gemini",
                DeploymentMode::Symlink,
            ))
            .unwrap();

        for targets in [
            store.get_targets_for_skill("skill-1").unwrap(),
            store.get_all_targets().unwrap(),
        ] {
            assert_eq!(targets.len(), 1);
            let t = &targets[0];
            assert_eq!(t.id, "target-1");
            assert_eq!(t.skill_id, "skill-1");
            assert_eq!(t.tool, "codex");
            assert_eq!(t.target_path, global_enabled.target_path);
            assert_eq!(t.mode, "symlink");
            assert_eq!(t.status, "ok");
            assert_eq!(t.synced_at, Some(1000));
            assert_eq!(t.last_error, None);
            assert_eq!(t.source_hash.as_deref(), Some("abc"));
        }
    }

    #[test]
    fn insert_target_writes_a_global_enabled_deployment() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");

        store
            .insert_target(&SkillTargetRecord {
                id: "target-1".to_string(),
                skill_id: "skill-1".to_string(),
                tool: "codex".to_string(),
                target_path: "/tmp/project/.agents/skills/demo".to_string(),
                mode: "symlink".to_string(),
                status: "ok".to_string(),
                synced_at: Some(1000),
                last_error: None,
                source_hash: Some("abc".to_string()),
            })
            .unwrap();

        let rows = store.get_deployments_for_artifact("skill-1").unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, "target-1");
        assert_eq!(row.scope, ArtifactScope::Global);
        assert_eq!(row.agent, "codex");
        assert!(row.enabled);
        assert_eq!(row.mode, DeploymentMode::Symlink);
        // The source path comes from the Skill's central path, not the caller.
        assert_eq!(row.source_path, "/tmp/library/skill-1");
        assert_eq!(row.target_path, "/tmp/project/.agents/skills/demo");
        assert_eq!(row.last_synced_hash.as_deref(), Some("abc"));
        assert_eq!(row.last_synced_at, Some(1000));
    }

    #[test]
    fn delete_target_removes_the_global_deployment_only() {
        let tmp = tempdir().unwrap();
        let store = store_with_skill(tmp.path(), "skill-1");
        store
            .upsert_deployment(&deployment(
                "d-global",
                "skill-1",
                ArtifactScope::Global,
                "codex",
                DeploymentMode::Symlink,
            ))
            .unwrap();
        store
            .upsert_deployment(&deployment(
                "d-project",
                "skill-1",
                ArtifactScope::Project("proj-1".to_string()),
                "codex",
                DeploymentMode::Symlink,
            ))
            .unwrap();

        store.delete_target("skill-1", "codex").unwrap();

        let rows = store.get_deployments_for_artifact("skill-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "d-project");
        assert!(store.get_targets_for_skill("skill-1").unwrap().is_empty());
    }
}

const DEPLOYMENT_COLUMNS: &str = "SELECT id, artifact_id, scope_type, scope_id, agent, enabled, mode,
            source_path, target_path, last_synced_hash, last_synced_at, status, last_error";

/// The legacy Skill target projection: global scope, enabled only.
const GLOBAL_TARGET_COLUMNS: &str = "SELECT id, artifact_id, agent, target_path, mode, status,
            last_synced_at, last_error, last_synced_hash
     FROM artifact_deployments
     WHERE scope_type = 'global' AND enabled = 1";

/// Ensure a kind `skill` Artifact identity exists for `id`.
///
/// `DO NOTHING` deliberately leaves a pre-existing row's kind alone: if the id
/// already belongs to another subtype, the Skill insert that follows trips the
/// kind trigger and the whole transaction aborts, rather than the Skill
/// silently stealing another Artifact's identity.
fn ensure_skill_artifact(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO artifacts (id, kind) VALUES (?1, 'skill') ON CONFLICT(id) DO NOTHING",
        params![id],
    )?;
    Ok(())
}

fn write_deployment(conn: &Connection, deployment: &ArtifactDeploymentRecord) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO artifact_deployments
            (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
             target_path, last_synced_hash, last_synced_at, status, last_error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            deployment.id,
            deployment.artifact_id,
            deployment.scope.scope_type(),
            deployment.scope.scope_id(),
            deployment.agent,
            deployment.enabled,
            deployment.mode.as_str(),
            deployment.source_path,
            deployment.target_path,
            deployment.last_synced_hash,
            deployment.last_synced_at,
            // Displayable state only — the same redaction the logs go through,
            // so a path or token in an error message never lands in the row.
            log_sanitize::sanitize(&deployment.status),
            deployment
                .last_error
                .as_deref()
                .map(log_sanitize::sanitize),
        ],
    )?;
    Ok(())
}

fn strict_column<T>(index: usize, parsed: Result<T>) -> rusqlite::Result<T> {
    parsed.map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e.into())
    })
}

fn map_deployment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactDeploymentRecord> {
    let scope_type: String = row.get(2)?;
    let scope_id: String = row.get(3)?;
    let mode: String = row.get(6)?;
    Ok(ArtifactDeploymentRecord {
        id: row.get(0)?,
        artifact_id: row.get(1)?,
        scope: strict_column(2, ArtifactScope::parse(&scope_type, &scope_id))?,
        agent: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        mode: strict_column(6, DeploymentMode::parse(&mode))?,
        source_path: row.get(7)?,
        target_path: row.get(8)?,
        last_synced_hash: row.get(9)?,
        last_synced_at: row.get(10)?,
        status: row.get(11)?,
        last_error: row.get(12)?,
    })
}

fn map_target_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillTargetRecord> {
    Ok(SkillTargetRecord {
        id: row.get(0)?,
        skill_id: row.get(1)?,
        tool: row.get(2)?,
        target_path: row.get(3)?,
        mode: row.get(4)?,
        status: row.get(5)?,
        synced_at: row.get(6)?,
        last_error: row.get(7)?,
        source_hash: row.get(8)?,
    })
}

fn map_skill_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        source_type: row.get(3)?,
        source_ref: row.get(4)?,
        source_ref_resolved: row.get(5)?,
        source_subpath: row.get(6)?,
        source_branch: row.get(7)?,
        source_revision: row.get(8)?,
        remote_revision: row.get(9)?,
        central_path: row.get(10)?,
        content_hash: row.get(11)?,
        enabled: row.get::<_, i32>(12)? != 0,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        status: row.get(15)?,
        update_status: row.get(16)?,
        last_checked_at: row.get(17)?,
        last_check_error: row.get(18)?,
    })
}

#[cfg(test)]
mod tag_tests {
    use super::*;
    use tempfile::tempdir;

    fn skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: format!("/tmp/{id}"),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        }
    }

    #[test]
    fn rename_tag_updates_all_and_merges_into_existing() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();
        store.insert_skill(&skill("b")).unwrap();
        store.set_tags_for_skill("a", &["old".into()]).unwrap();
        // b already carries the target name, so the rename must merge, not dup.
        store
            .set_tags_for_skill("b", &["old".into(), "new".into()])
            .unwrap();

        let mut affected = store.rename_tag("old", "new").unwrap();
        affected.sort();
        assert_eq!(affected, vec!["a".to_string(), "b".to_string()]);

        assert_eq!(store.get_all_tags().unwrap(), vec!["new".to_string()]);
        let map = store.get_tags_map().unwrap();
        assert_eq!(map.get("a").unwrap(), &vec!["new".to_string()]);
        assert_eq!(map.get("b").unwrap(), &vec!["new".to_string()]);
    }

    #[test]
    fn rename_tag_to_itself_is_noop_not_delete() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();
        store.set_tags_for_skill("a", &["keep".into()]).unwrap();

        let affected = store.rename_tag("keep", "keep").unwrap();
        assert_eq!(affected, vec!["a".to_string()]);
        // The tag must survive a self-rename, not be wiped.
        assert_eq!(store.get_all_tags().unwrap(), vec!["keep".to_string()]);
    }

    #[test]
    fn delete_tag_removes_from_all_skills() {
        let tmp = tempdir().unwrap();
        let store = SkillStore::new(&tmp.path().join("test.db")).unwrap();
        store.insert_skill(&skill("a")).unwrap();
        store.insert_skill(&skill("b")).unwrap();
        store
            .set_tags_for_skill("a", &["keep".into(), "drop".into()])
            .unwrap();
        store.set_tags_for_skill("b", &["drop".into()]).unwrap();

        let mut affected = store.delete_tag("drop").unwrap();
        affected.sort();
        assert_eq!(affected, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(store.get_all_tags().unwrap(), vec!["keep".to_string()]);
        let map = store.get_tags_map().unwrap();
        assert_eq!(map.get("a").unwrap(), &vec!["keep".to_string()]);
        assert!(map.get("b").is_none());
    }
}
