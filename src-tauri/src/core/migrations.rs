use anyhow::{bail, Context, Result};
use rusqlite::Connection;

/// Current schema version. Bump this when adding a new migration.
const LATEST_VERSION: u32 = 10;

/// Run all pending migrations on the database.
///
/// - New databases: creates full schema and sets version to LATEST_VERSION.
/// - Existing databases (user_version == 0): runs incremental migrations
///   to bring them up to date.
/// - Databases newer than this app version: returns an error.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if current > LATEST_VERSION {
        bail!(
            "Database schema version ({current}) is newer than this app supports ({LATEST_VERSION}). \
             Please upgrade the application."
        );
    }

    if current == LATEST_VERSION {
        return Ok(());
    }

    // Run each migration step in a transaction
    for version in current..LATEST_VERSION {
        conn.execute_batch("BEGIN EXCLUSIVE")?;
        match migrate_step(conn, version) {
            Ok(()) => {
                conn.pragma_update(None, "user_version", version + 1)?;
                conn.execute_batch("COMMIT")?;
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e).with_context(|| {
                    format!("migration from version {version} to {} failed", version + 1)
                });
            }
        }
    }

    Ok(())
}

/// Execute a single migration step: version N → N+1.
fn migrate_step(conn: &Connection, from_version: u32) -> Result<()> {
    match from_version {
        0 => migrate_v0_to_v1(conn),
        1 => migrate_v1_to_v2(conn),
        2 => migrate_v2_to_v3(conn),
        3 => migrate_v3_to_v4(conn),
        4 => migrate_v4_to_v5(conn),
        5 => migrate_v5_to_v6(conn),
        6 => migrate_v6_to_v7(conn),
        7 => migrate_v7_to_v8(conn),
        8 => migrate_v8_to_v9(conn),
        9 => migrate_v9_to_v10(conn),
        _ => bail!("unknown migration version: {from_version}"),
    }
}

/// v0 → v1: Initial schema.
///
/// For new databases this creates all tables from scratch.
/// For existing pre-migration databases, the `CREATE TABLE IF NOT EXISTS`
/// statements are no-ops, and the `add_column_if_missing` calls handle
/// columns that were added incrementally before the migration system existed.
fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS skills (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            source_type TEXT NOT NULL,
            source_ref TEXT,
            source_ref_resolved TEXT,
            source_subpath TEXT,
            source_branch TEXT,
            source_revision TEXT,
            remote_revision TEXT,
            central_path TEXT NOT NULL UNIQUE,
            content_hash TEXT,
            enabled INTEGER DEFAULT 1,
            created_at INTEGER,
            updated_at INTEGER,
            status TEXT DEFAULT 'ok',
            update_status TEXT DEFAULT 'unknown',
            last_checked_at INTEGER,
            last_check_error TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);

        CREATE TABLE IF NOT EXISTS skill_targets (
            id TEXT PRIMARY KEY,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            tool TEXT NOT NULL,
            target_path TEXT NOT NULL,
            mode TEXT NOT NULL,
            status TEXT DEFAULT 'ok',
            synced_at INTEGER,
            last_error TEXT,
            source_hash TEXT,
            UNIQUE(skill_id, tool)
        );

        CREATE TABLE IF NOT EXISTS discovered_skills (
            id TEXT PRIMARY KEY,
            tool TEXT NOT NULL,
            found_path TEXT NOT NULL,
            name_guess TEXT,
            fingerprint TEXT,
            found_at INTEGER NOT NULL,
            imported_skill_id TEXT REFERENCES skills(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS skillssh_cache (
            cache_key TEXT PRIMARY KEY,
            data TEXT NOT NULL,
            fetched_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS scenarios (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT,
            icon TEXT,
            sort_order INTEGER DEFAULT 0,
            created_at INTEGER,
            updated_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS scenario_skills (
            scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            added_at INTEGER,
            PRIMARY KEY(scenario_id, skill_id)
        );

        CREATE TABLE IF NOT EXISTS scenario_skill_tools (
            scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            tool TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY(scenario_id, skill_id, tool)
        );

        CREATE TABLE IF NOT EXISTS active_scenario (
            key TEXT PRIMARY KEY DEFAULT 'current',
            scenario_id TEXT REFERENCES scenarios(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            workspace_type TEXT NOT NULL DEFAULT 'project',
            linked_agent_key TEXT,
            linked_agent_name TEXT,
            disabled_path TEXT,
            sort_order INTEGER DEFAULT 0,
            created_at INTEGER,
            updated_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS skill_tags (
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            tag TEXT NOT NULL,
            PRIMARY KEY(skill_id, tag)
        );
        CREATE INDEX IF NOT EXISTS idx_skill_tags_tag ON skill_tags(tag);
        ",
    )?;

    // For pre-migration databases: add columns that didn't exist in the original schema.
    // For new databases these are already in the CREATE TABLE, so the calls are no-ops.
    add_column_if_missing(conn, "scenarios", "icon", "TEXT")?;
    add_column_if_missing(conn, "skills", "source_ref_resolved", "TEXT")?;
    add_column_if_missing(conn, "skills", "source_subpath", "TEXT")?;
    add_column_if_missing(conn, "skills", "source_branch", "TEXT")?;
    add_column_if_missing(conn, "skills", "remote_revision", "TEXT")?;
    add_column_if_missing(conn, "skills", "update_status", "TEXT DEFAULT 'unknown'")?;
    add_column_if_missing(conn, "skills", "last_checked_at", "INTEGER")?;
    add_column_if_missing(conn, "skills", "last_check_error", "TEXT")?;

    Ok(())
}

/// v1 → v2: Add per-scenario, per-skill tool toggle table.
fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS scenario_skill_tools (
            scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
            skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            tool TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY(scenario_id, skill_id, tool)
        );
        ",
    )?;
    Ok(())
}

/// v2 → v3: Add sort_order to scenario_skills for drag-and-drop reordering.
fn migrate_v2_to_v3(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "scenario_skills", "sort_order", "INTEGER DEFAULT 0")?;
    Ok(())
}

/// v3 → v4: Expand projects into generic workspace records.
fn migrate_v3_to_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            workspace_type TEXT NOT NULL DEFAULT 'project',
            linked_agent_key TEXT,
            linked_agent_name TEXT,
            disabled_path TEXT,
            sort_order INTEGER DEFAULT 0,
            created_at INTEGER,
            updated_at INTEGER
        );
        ",
    )?;
    add_column_if_missing(
        conn,
        "projects",
        "workspace_type",
        "TEXT NOT NULL DEFAULT 'project'",
    )?;
    add_column_if_missing(conn, "projects", "linked_agent_key", "TEXT")?;
    add_column_if_missing(conn, "projects", "linked_agent_name", "TEXT")?;
    add_column_if_missing(conn, "projects", "disabled_path", "TEXT")?;
    Ok(())
}

/// v4 → v5: Add audit log table — append-only history of user/system actions.
fn migrate_v4_to_v5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            action TEXT NOT NULL,
            skill_id TEXT,
            skill_name TEXT,
            tool TEXT,
            success INTEGER NOT NULL,
            detail TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON audit_log(ts);
        ",
    )?;
    Ok(())
}

/// v5 → v6: Add `source_hash` to `skill_targets`. Lets the sync engine
/// skip a Copy-mode resync when the central skill content has not
/// changed since the last successful sync, avoiding the per-startup
/// recursive copy that pinned Windows users on issue #153.
///
/// Existing rows get NULL, which is treated as "no recorded hash" and
/// forces one copy on the first post-upgrade sync. No backfill needed.
fn migrate_v5_to_v6(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "skill_targets", "source_hash", "TEXT")?;
    Ok(())
}

/// v6 → v7: pending-conflict projection for the object merge engine
/// (merge-engine design §4). A local UI cache only — the source of truth is
/// the commit trailers plus `refs/skills-manager/conflict/*`, from which
/// this table is rebuilt at startup and after every merge.
fn migrate_v6_to_v7(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS pending_conflicts (
            skill_id TEXT PRIMARY KEY,
            theirs_commit TEXT NOT NULL,
            theirs_path TEXT,
            detected_at INTEGER NOT NULL
        );
        ",
    )?;
    Ok(())
}

/// v7 → v8: Artifact foundation.
///
/// Splits identity from subtype detail: every Skill keeps its id and gains a
/// kind `skill` Artifact parent, and `skill_targets` becomes the generic
/// `artifact_deployments` table that can also express project scope and
/// CLI-managed deployments. The whole step runs inside the migration runner's
/// transaction, so a database is only ever complete v7 or complete v8 — the
/// legacy table is dropped last, after the invariants have been verified.
fn migrate_v7_to_v8(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE artifacts (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL CHECK(kind IN ('skill', 'plugin', 'hook', 'config_profile'))
        );

        CREATE TABLE artifact_deployments (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
            scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'project')),
            scope_id TEXT NOT NULL,
            agent TEXT NOT NULL,
            enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
            mode TEXT NOT NULL CHECK(mode IN ('symlink', 'copy', 'cli-managed')),
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            last_synced_hash TEXT,
            last_synced_at INTEGER,
            status TEXT NOT NULL,
            last_error TEXT,
            UNIQUE(artifact_id, scope_type, scope_id, agent),
            CHECK(
                (scope_type = 'global' AND scope_id = '')
                OR (scope_type = 'project' AND scope_id <> '')
            )
        );
        CREATE INDEX idx_artifact_deployments_artifact ON artifact_deployments(artifact_id);
        CREATE INDEX idx_artifact_deployments_agent ON artifact_deployments(agent);
        ",
    )?;

    // Identity backfill: the Skill id becomes the Artifact id, so nothing that
    // already references a skill id has to be rewritten.
    conn.execute(
        "INSERT INTO artifacts (id, kind) SELECT id, 'skill' FROM skills",
        [],
    )?;

    // SQLite cannot add a NOT NULL column to an existing table, so the column
    // starts nullable and the invariant is carried by the backfill plus the
    // unique index and triggers below.
    add_column_if_missing(
        conn,
        "skills",
        "artifact_id",
        "TEXT REFERENCES artifacts(id) ON DELETE CASCADE",
    )?;
    conn.execute("UPDATE skills SET artifact_id = id", [])?;

    conn.execute_batch(
        "
        CREATE UNIQUE INDEX idx_skills_artifact_id ON skills(artifact_id);

        CREATE TRIGGER trg_skills_require_skill_artifact_insert
        BEFORE INSERT ON skills
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'skill'
        BEGIN
            SELECT RAISE(ABORT, 'skills.artifact_id must reference an artifact of kind skill');
        END;

        CREATE TRIGGER trg_skills_require_skill_artifact_update
        BEFORE UPDATE OF artifact_id ON skills
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'skill'
        BEGIN
            SELECT RAISE(ABORT, 'skills.artifact_id must reference an artifact of kind skill');
        END;

        CREATE TRIGGER trg_skills_drop_artifact_identity
        AFTER DELETE ON skills
        FOR EACH ROW
        BEGIN
            DELETE FROM artifacts WHERE id = OLD.artifact_id;
        END;
        ",
    )?;

    // Legacy targets were always global and always active. The join drops any
    // target whose Skill is gone; the count check below turns that into an
    // explicit failure rather than silent data loss.
    conn.execute(
        "INSERT INTO artifact_deployments
            (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
             target_path, last_synced_hash, last_synced_at, status, last_error)
         SELECT t.id, t.skill_id, 'global', '', t.tool, 1, t.mode, s.central_path,
                t.target_path, t.source_hash, t.synced_at, COALESCE(t.status, 'ok'), t.last_error
         FROM skill_targets t
         JOIN skills s ON s.id = t.skill_id",
        [],
    )?;

    verify_v8_invariants(conn)?;

    // Only now is the legacy table redundant.
    conn.execute("DROP TABLE skill_targets", [])?;

    Ok(())
}

/// v8 → v9: Hook identity and recovery metadata.
///
/// Pure DDL: no existing row is read or rewritten, and nothing is seeded. A
/// managed Hook source only gets an identity when a write actually succeeds,
/// so a user who never edits a Hook keeps an empty pair of tables.
///
/// Neither table has a column for Hook content or for the source path. Content
/// would put commands, prompts, URLs and headers into a file that is backed up
/// and synced; the path is re-derived from the fixed descriptor and the Project
/// record on every use, so it cannot go stale.
fn migrate_v8_to_v9(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE hook_details (
            artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id) ON DELETE CASCADE,
            source_id TEXT NOT NULL,
            context_key TEXT NOT NULL,
            agent TEXT NOT NULL,
            scope TEXT NOT NULL,
            format TEXT NOT NULL CHECK(format IN ('json', 'toml')),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(source_id, context_key),
            CHECK(context_key = 'global' OR context_key LIKE 'project:_%')
        );

        CREATE TABLE hook_backups (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('bytes', 'absent')),
            before_hash TEXT NOT NULL,
            after_hash TEXT NOT NULL,
            locator TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            restored_at INTEGER,
            UNIQUE(artifact_id)
        );

        CREATE INDEX idx_hook_details_source ON hook_details(source_id, context_key);

        -- A CHECK cannot reach the parent row, so the kind constraint that makes
        -- these detail tables belong to Hook Artifacts is carried by triggers.
        CREATE TRIGGER hook_details_require_hook_kind_insert
        BEFORE INSERT ON hook_details
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'hook'
        BEGIN
            SELECT RAISE(ABORT, 'hook detail requires a kind hook artifact');
        END;

        CREATE TRIGGER hook_details_require_hook_kind_update
        BEFORE UPDATE ON hook_details
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'hook'
        BEGIN
            SELECT RAISE(ABORT, 'hook detail requires a kind hook artifact');
        END;

        CREATE TRIGGER hook_backups_require_hook_kind_insert
        BEFORE INSERT ON hook_backups
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'hook'
        BEGIN
            SELECT RAISE(ABORT, 'hook backup requires a kind hook artifact');
        END;
        ",
    )?;
    Ok(())
}

/// v9 → v10: Config Profile typed persistence and recovery metadata.
///
/// Three new tables plus one widened CHECK on `artifact_deployments`. No
/// existing row is seeded or rewritten: a profile only exists once the user
/// saves one, and an assignment only exists once the user makes one.
///
/// The entry table spends three mutually exclusive columns on what a single
/// blob column could hold, because a blob would let an unknown table, an array
/// or a credential into a row that the whole capability treats as "one exact
/// allowlisted scalar". The CHECK is the enforcement, not a convention.
///
/// No table has a column for a source path or for document bytes. The path is
/// re-derived from the fixed descriptor and the Project record on every use,
/// and the recovery payload lives in owner-private app state that neither the
/// Library nor Git backup reaches.
fn migrate_v9_to_v10(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE config_profile_details (
            artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id) ON DELETE CASCADE,
            name TEXT NOT NULL UNIQUE,
            revision INTEGER NOT NULL CHECK(revision >= 1),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            CHECK(name <> '')
        );

        CREATE TABLE config_profile_entries (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL
                REFERENCES config_profile_details(artifact_id) ON DELETE CASCADE,
            agent TEXT NOT NULL CHECK(agent IN ('codex', 'claude_code')),
            canonical_key TEXT NOT NULL CHECK(canonical_key <> ''),
            value_type TEXT NOT NULL CHECK(value_type IN ('string', 'boolean', 'integer')),
            string_value TEXT,
            boolean_value INTEGER,
            integer_value INTEGER,
            UNIQUE(artifact_id, agent, canonical_key),
            CHECK(boolean_value IS NULL OR boolean_value IN (0, 1)),
            -- Exactly one typed column, and it is the one `value_type` names.
            CHECK(
                (value_type = 'string'
                    AND string_value IS NOT NULL
                    AND boolean_value IS NULL
                    AND integer_value IS NULL)
                OR (value_type = 'boolean'
                    AND boolean_value IS NOT NULL
                    AND string_value IS NULL
                    AND integer_value IS NULL)
                OR (value_type = 'integer'
                    AND integer_value IS NOT NULL
                    AND string_value IS NULL
                    AND boolean_value IS NULL)
            )
        );

        CREATE TABLE config_profile_recoveries (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
            project_id TEXT NOT NULL CHECK(project_id <> ''),
            agent TEXT NOT NULL CHECK(agent IN ('codex', 'claude_code')),
            source_id TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('bytes', 'absent')),
            before_hash TEXT NOT NULL,
            after_hash TEXT NOT NULL,
            locator TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision >= 1),
            created_at INTEGER NOT NULL,
            -- Only the latest recovery point per assignment is ever exposed, so
            -- the assignment tuple is the primary identity of the row.
            UNIQUE(artifact_id, project_id, agent),
            CHECK((kind = 'absent' AND locator = '') OR (kind = 'bytes' AND locator <> ''))
        );

        CREATE INDEX idx_config_profile_entries_profile
            ON config_profile_entries(artifact_id, agent);

        -- A CHECK cannot reach the parent row, so the kind constraint that makes
        -- these detail tables belong to Config Profile Artifacts is carried by
        -- triggers, exactly as the Hook detail tables carry theirs.
        CREATE TRIGGER config_profile_details_require_kind_insert
        BEFORE INSERT ON config_profile_details
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'config_profile'
        BEGIN
            SELECT RAISE(ABORT, 'config profile detail requires a kind config_profile artifact');
        END;

        CREATE TRIGGER config_profile_details_require_kind_update
        BEFORE UPDATE ON config_profile_details
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'config_profile'
        BEGIN
            SELECT RAISE(ABORT, 'config profile detail requires a kind config_profile artifact');
        END;

        CREATE TRIGGER config_profile_recoveries_require_kind_insert
        BEFORE INSERT ON config_profile_recoveries
        FOR EACH ROW
        WHEN (SELECT kind FROM artifacts WHERE id = NEW.artifact_id) IS NOT 'config_profile'
        BEGIN
            SELECT RAISE(ABORT, 'config profile recovery requires a kind config_profile artifact');
        END;
        ",
    )?;

    widen_deployment_modes(conn)?;
    Ok(())
}

/// Adds `config-profile` to the deployment mode vocabulary.
///
/// SQLite cannot alter a CHECK in place, so the table is rebuilt. The rebuild
/// copies every column verbatim and the row count is verified before the old
/// table goes, which keeps "assignments reuse canonical deployments" from
/// costing an existing Skill or Plugin deployment.
fn widen_deployment_modes(conn: &Connection) -> Result<()> {
    let before: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifact_deployments",
        [],
        |row| row.get(0),
    )?;

    conn.execute_batch(
        "
        CREATE TABLE artifact_deployments_v10 (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
            scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'project')),
            scope_id TEXT NOT NULL,
            agent TEXT NOT NULL,
            enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
            mode TEXT NOT NULL CHECK(mode IN ('symlink', 'copy', 'cli-managed', 'config-profile')),
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            last_synced_hash TEXT,
            last_synced_at INTEGER,
            status TEXT NOT NULL,
            last_error TEXT,
            UNIQUE(artifact_id, scope_type, scope_id, agent),
            CHECK(
                (scope_type = 'global' AND scope_id = '')
                OR (scope_type = 'project' AND scope_id <> '')
            )
        );

        INSERT INTO artifact_deployments_v10
            (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
             target_path, last_synced_hash, last_synced_at, status, last_error)
        SELECT id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
               target_path, last_synced_hash, last_synced_at, status, last_error
        FROM artifact_deployments;
        ",
    )?;

    let copied: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifact_deployments_v10",
        [],
        |row| row.get(0),
    )?;
    if copied != before {
        bail!("deployment rebuild count mismatch: {before} before, {copied} copied");
    }

    conn.execute_batch(
        "
        DROP TABLE artifact_deployments;
        ALTER TABLE artifact_deployments_v10 RENAME TO artifact_deployments;
        CREATE INDEX idx_artifact_deployments_artifact ON artifact_deployments(artifact_id);
        CREATE INDEX idx_artifact_deployments_agent ON artifact_deployments(agent);
        ",
    )?;

    Ok(())
}

/// Verify the v8 backfill before the legacy table is dropped. Every failure
/// names the invariant so a rollback is diagnosable from the error alone.
fn verify_v8_invariants(conn: &Connection) -> Result<()> {
    let skills: i64 = conn.query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))?;
    let artifacts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifacts WHERE kind = 'skill'",
        [],
        |row| row.get(0),
    )?;
    if artifacts != skills {
        bail!("artifact identity count mismatch: {skills} skills, {artifacts} skill artifacts");
    }

    let unlinked: i64 = conn.query_row(
        "SELECT COUNT(*) FROM skills WHERE artifact_id IS NULL OR artifact_id <> id",
        [],
        |row| row.get(0),
    )?;
    if unlinked != 0 {
        bail!("artifact identity link mismatch: {unlinked} skills are not linked to their own artifact");
    }

    let legacy: i64 = conn.query_row("SELECT COUNT(*) FROM skill_targets", [], |row| row.get(0))?;
    let deployments: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifact_deployments",
        [],
        |row| row.get(0),
    )?;
    if deployments != legacy {
        bail!("deployment backfill count mismatch: {legacy} legacy targets, {deployments} migrated deployments");
    }

    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let violations: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !violations.is_empty() {
        bail!("foreign key integrity check failed after artifact backfill: {violations:?}");
    }

    Ok(())
}

/// Build a genuine schema v7 database by replaying migrations 0..7, so tests
/// that need a pre-Artifact database cannot drift from the real thing.
#[cfg(test)]
pub(crate) fn create_v7_schema(conn: &Connection) -> Result<()> {
    for version in 0..7 {
        migrate_step(conn, version)?;
    }
    conn.pragma_update(None, "user_version", 7)?;
    Ok(())
}

// ── Helpers ──

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    // Validate identifiers to prevent SQL injection if call sites ever change.
    validate_identifier(table)?;
    validate_identifier(column)?;

    if !has_column(conn, table, column)? {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn validate_identifier(name: &str) -> Result<()> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!("Invalid SQL identifier: {}", name);
    }
    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns.iter().any(|name| name == column))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_database_migrates_to_latest() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"skills".to_string()));
        assert!(tables.contains(&"artifacts".to_string()));
        assert!(tables.contains(&"artifact_deployments".to_string()));
        // Replaced by `artifact_deployments` in schema v8.
        assert!(!tables.contains(&"skill_targets".to_string()));
        assert!(tables.contains(&"scenarios".to_string()));
        assert!(tables.contains(&"projects".to_string()));
        assert!(tables.contains(&"skill_tags".to_string()));
        assert!(tables.contains(&"scenario_skill_tools".to_string()));
        assert!(tables.contains(&"audit_log".to_string()));
    }

    #[test]
    fn test_idempotent_migration() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        run_migrations(&conn).unwrap();
        // Running again should be a no-op
        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn test_pre_migration_database_upgrades() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Simulate a pre-migration database: create skills table without newer columns
        conn.execute_batch(
            "
            CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                source_type TEXT NOT NULL,
                source_ref TEXT,
                source_revision TEXT,
                central_path TEXT NOT NULL UNIQUE,
                content_hash TEXT,
                enabled INTEGER DEFAULT 1,
                created_at INTEGER,
                updated_at INTEGER,
                status TEXT DEFAULT 'ok'
            );
            CREATE TABLE scenarios (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                sort_order INTEGER DEFAULT 0,
                created_at INTEGER,
                updated_at INTEGER
            );
            ",
        )
        .unwrap();

        // user_version is 0 (default), so migration should run
        run_migrations(&conn).unwrap();

        // Verify new columns were added
        assert!(has_column(&conn, "skills", "source_ref_resolved").unwrap());
        assert!(has_column(&conn, "skills", "source_subpath").unwrap());
        assert!(has_column(&conn, "skills", "source_branch").unwrap());
        assert!(has_column(&conn, "skills", "remote_revision").unwrap());
        assert!(has_column(&conn, "skills", "update_status").unwrap());
        assert!(has_column(&conn, "skills", "last_checked_at").unwrap());
        assert!(has_column(&conn, "skills", "last_check_error").unwrap());
        assert!(has_column(&conn, "scenarios", "icon").unwrap());

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn test_v1_database_upgrades_to_v2() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        conn.execute_batch(
            "
            CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                source_type TEXT NOT NULL,
                source_ref TEXT,
                source_ref_resolved TEXT,
                source_subpath TEXT,
                source_branch TEXT,
                source_revision TEXT,
                remote_revision TEXT,
                central_path TEXT NOT NULL UNIQUE,
                content_hash TEXT,
                enabled INTEGER DEFAULT 1,
                created_at INTEGER,
                updated_at INTEGER,
                status TEXT DEFAULT 'ok',
                update_status TEXT DEFAULT 'unknown',
                last_checked_at INTEGER,
                last_check_error TEXT
            );
            CREATE TABLE scenarios (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                icon TEXT,
                sort_order INTEGER DEFAULT 0,
                created_at INTEGER,
                updated_at INTEGER
            );
            CREATE TABLE scenario_skills (
                scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
                skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
                added_at INTEGER,
                PRIMARY KEY(scenario_id, skill_id)
            );
            CREATE TABLE skill_targets (
                id TEXT PRIMARY KEY,
                skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
                tool TEXT NOT NULL,
                target_path TEXT NOT NULL,
                mode TEXT NOT NULL,
                status TEXT DEFAULT 'ok',
                synced_at INTEGER,
                last_error TEXT,
                UNIQUE(skill_id, tool)
            );
            PRAGMA user_version = 1;
            ",
        )
        .unwrap();

        run_migrations(&conn).unwrap();
        assert!(has_column(&conn, "scenario_skill_tools", "enabled").unwrap());
        // v5→v6 added `skill_targets.source_hash`; v7→v8 carries it forward as
        // `artifact_deployments.last_synced_hash`.
        assert!(has_column(&conn, "artifact_deployments", "last_synced_hash").unwrap());

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    // ── Schema v8 (Artifact foundation) ──

    fn create_v7_schema(conn: &Connection) {
        super::create_v7_schema(conn).unwrap();
    }

    /// Values are the ones fixed by the spec examples for the legacy target
    /// mapping, so a change in the mapping shows up as a value diff.
    fn populate_v7_fixture(conn: &Connection) {
        conn.execute_batch(
            "
            INSERT INTO skills (id, name, description, source_type, source_ref, source_ref_resolved,
                                source_subpath, source_branch, source_revision, remote_revision,
                                central_path, content_hash, enabled, created_at, updated_at, status,
                                update_status, last_checked_at, last_check_error)
            VALUES ('skill-1', 'Demo', 'demo skill', 'git', 'https://example.com/demo', 'main',
                    'skills/demo', 'main', 'rev1', 'rev2', '/tmp/library/demo', 'hash1', 1, 10, 11,
                    'ok', 'up_to_date', 12, NULL),
                   ('skill-2', 'Other', NULL, 'import', NULL, NULL, NULL, NULL, NULL, NULL,
                    '/tmp/library/other', NULL, 0, 20, 21, 'ok', 'local_only', NULL, 'boom');

            INSERT INTO skill_targets (id, skill_id, tool, target_path, mode, status, synced_at, last_error, source_hash)
            VALUES ('target-1', 'skill-1', 'codex', '/tmp/project/.agents/skills/demo', 'symlink', 'ok', 1000, NULL, 'abc'),
                   ('target-2', 'skill-2', 'claude', '/tmp/project/.claude/skills/other', 'copy', 'error', 2000, 'sync failed', 'def');

            INSERT INTO skill_tags (skill_id, tag) VALUES ('skill-1', 'alpha'), ('skill-1', 'beta'), ('skill-2', 'alpha');

            INSERT INTO scenarios (id, name, description, icon, sort_order, created_at, updated_at)
            VALUES ('sc-1', 'Work', 'work setup', 'briefcase', 0, 30, 31);

            INSERT INTO scenario_skills (scenario_id, skill_id, added_at, sort_order)
            VALUES ('sc-1', 'skill-1', 40, 0);

            INSERT INTO scenario_skill_tools (scenario_id, skill_id, tool, enabled, updated_at)
            VALUES ('sc-1', 'skill-1', 'codex', 1, 41), ('sc-1', 'skill-1', 'claude', 0, 42);

            INSERT INTO active_scenario (key, scenario_id) VALUES ('current', 'sc-1');

            INSERT INTO projects (id, name, path, workspace_type, linked_agent_key, linked_agent_name,
                                  disabled_path, sort_order, created_at, updated_at)
            VALUES ('proj-1', 'Demo Project', '/tmp/project', 'project', NULL, NULL, NULL, 0, 50, 51);

            INSERT INTO settings (key, value) VALUES ('theme', 'dark');

            INSERT INTO discovered_skills (id, tool, found_path, name_guess, fingerprint, found_at, imported_skill_id)
            VALUES ('disc-1', 'codex', '/tmp/found/demo', 'demo', 'fp1', 60, 'skill-1');

            INSERT INTO audit_log (ts, action, skill_id, skill_name, tool, success, detail)
            VALUES (70, 'install', 'skill-1', 'Demo', 'codex', 1, 'ok');

            INSERT INTO pending_conflicts (skill_id, theirs_commit, theirs_path, detected_at)
            VALUES ('skill-2', 'deadbeef', '/tmp/theirs/other', 80);
            ",
        )
        .unwrap();
    }

    /// Every row of `table`, one string per row, sorted for stable comparison.
    fn dump_table(conn: &Connection, table: &str) -> Vec<String> {
        dump_query(conn, &format!("SELECT * FROM {table}"))
    }

    fn dump_query(conn: &Connection, sql: &str) -> Vec<String> {
        let mut stmt = conn.prepare(sql).unwrap();
        let cols = stmt.column_count();
        let mut rows: Vec<String> = stmt
            .query_map([], |row| {
                let mut parts = Vec::with_capacity(cols);
                for i in 0..cols {
                    parts.push(format!("{:?}", row.get::<_, rusqlite::types::Value>(i)?));
                }
                Ok(parts.join("|"))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        rows.sort();
        rows
    }

    fn schema_snapshot(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT type, name, COALESCE(sql, '') FROM sqlite_master ORDER BY type, name")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(format!(
                "{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    fn table_exists(conn: &Connection, table: &str) -> bool {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        count > 0
    }

    fn scalar(conn: &Connection, sql: &str) -> String {
        conn.query_row(sql, [], |row| {
            Ok(format!("{:?}", row.get::<_, rusqlite::types::Value>(0)?))
        })
        .unwrap()
    }

    #[test]
    fn test_populated_v7_upgrades_without_data_loss() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_v7_schema(&conn);
        populate_v7_fixture(&conn);

        // `skills` gains `artifact_id`, so it is compared over its v7 columns
        // separately; every other table must come out untouched.
        const V7_SKILL_COLUMNS: &str =
            "SELECT id, name, description, source_type, source_ref, source_ref_resolved,
                    source_subpath, source_branch, source_revision, remote_revision, central_path,
                    content_hash, enabled, created_at, updated_at, status, update_status,
                    last_checked_at, last_check_error
             FROM skills";
        let skills_before = dump_query(&conn, V7_SKILL_COLUMNS);

        let untouched = [
            "skill_tags",
            "scenarios",
            "scenario_skills",
            "scenario_skill_tools",
            "active_scenario",
            "projects",
            "settings",
            "discovered_skills",
            "audit_log",
            "pending_conflicts",
        ];
        let before: Vec<Vec<String>> = untouched.iter().map(|t| dump_table(&conn, t)).collect();
        let legacy_targets = dump_table(&conn, "skill_targets");

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);

        for (table, expected) in untouched.iter().zip(before.iter()) {
            assert_eq!(&dump_table(&conn, table), expected, "table {table} changed");
        }
        assert_eq!(dump_query(&conn, V7_SKILL_COLUMNS), skills_before);

        // One kind `skill` Artifact per Skill, sharing the Skill id.
        assert_eq!(
            dump_table(&conn, "artifacts"),
            vec![
                "Text(\"skill-1\")|Text(\"skill\")".to_string(),
                "Text(\"skill-2\")|Text(\"skill\")".to_string(),
            ]
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM skills WHERE artifact_id IS NULL"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM skills WHERE artifact_id <> id"),
            "Integer(0)"
        );

        // Legacy target `target-1` maps field-by-field onto a global enabled
        // deployment, with the source path taken from the Skill central path.
        let row = conn
            .query_row(
                "SELECT artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
                        target_path, last_synced_hash, last_synced_at, status, last_error
                 FROM artifact_deployments WHERE id = 'target-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, Option<String>>(11)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "skill-1".to_string(),
                "global".to_string(),
                String::new(),
                "codex".to_string(),
                1,
                "symlink".to_string(),
                "/tmp/library/demo".to_string(),
                "/tmp/project/.agents/skills/demo".to_string(),
                Some("abc".to_string()),
                Some(1000),
                "ok".to_string(),
                None,
            )
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM artifact_deployments"),
            format!("Integer({})", legacy_targets.len())
        );

        // Legacy storage only disappears on the success path.
        assert!(!table_exists(&conn, "skill_targets"));
    }

    #[test]
    fn test_fresh_database_reaches_schema_v8() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
        assert!(table_exists(&conn, "artifacts"));
        assert!(table_exists(&conn, "artifact_deployments"));
        assert!(!table_exists(&conn, "skill_targets"));
        assert!(has_column(&conn, "skills", "artifact_id").unwrap());
        // No seed rows.
        assert_eq!(scalar(&conn, "SELECT COUNT(*) FROM artifacts"), "Integer(0)");
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM artifact_deployments"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_fresh_and_upgraded_schemas_match() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&fresh).unwrap();

        let upgraded = Connection::open_in_memory().unwrap();
        upgraded.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_v7_schema(&upgraded);
        populate_v7_fixture(&upgraded);
        run_migrations(&upgraded).unwrap();

        assert_eq!(schema_snapshot(&fresh), schema_snapshot(&upgraded));
    }

    #[test]
    fn test_v8_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_v7_schema(&conn);
        populate_v7_fixture(&conn);
        run_migrations(&conn).unwrap();

        let schema = schema_snapshot(&conn);
        let artifacts = dump_table(&conn, "artifacts");
        let deployments = dump_table(&conn, "artifact_deployments");

        run_migrations(&conn).unwrap();

        assert_eq!(schema_snapshot(&conn), schema);
        assert_eq!(dump_table(&conn, "artifacts"), artifacts);
        assert_eq!(dump_table(&conn, "artifact_deployments"), deployments);
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn test_migration_verification_failure_rolls_back() {
        let conn = Connection::open_in_memory().unwrap();
        // Foreign keys off while seeding lets the fixture carry a target row
        // whose Skill is gone — the corruption the count check must catch.
        conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        create_v7_schema(&conn);
        populate_v7_fixture(&conn);
        conn.execute(
            "INSERT INTO skill_targets (id, skill_id, tool, target_path, mode, status, synced_at, last_error, source_hash)
             VALUES ('target-orphan', 'skill-missing', 'codex', '/tmp/orphan', 'symlink', 'ok', 3000, NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        let schema_before = schema_snapshot(&conn);
        let skills_before = dump_table(&conn, "skills");
        let targets_before = dump_table(&conn, "skill_targets");

        // `{:#}` renders the whole anyhow chain, not just the runner's context.
        let err = format!("{:#}", run_migrations(&conn).unwrap_err());
        assert!(
            err.contains("deployment backfill count mismatch"),
            "error should name the failed invariant, got: {err}"
        );

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 7);
        assert_eq!(schema_snapshot(&conn), schema_before);
        assert_eq!(dump_table(&conn, "skills"), skills_before);
        assert_eq!(dump_table(&conn, "skill_targets"), targets_before);
        assert!(!table_exists(&conn, "artifacts"));
        assert!(!table_exists(&conn, "artifact_deployments"));
        assert!(!has_column(&conn, "skills", "artifact_id").unwrap());
    }

    #[test]
    fn test_newer_schema_rejected() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();

        let err = run_migrations(&conn).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("newer than this app supports"),
            "unexpected error: {msg}"
        );
    }

    // ── Schema v9: Hook identity and recovery metadata ──

    /// A database that has been migrated up to v8 and stopped there, which is
    /// what an installed copy of the previous release holds.
    fn populated_v8(conn: &Connection) {
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_v7_schema(conn);
        populate_v7_fixture(conn);
        for version in 7..8 {
            conn.execute_batch("BEGIN EXCLUSIVE").unwrap();
            migrate_step(conn, version).unwrap();
            conn.pragma_update(None, "user_version", version + 1).unwrap();
            conn.execute_batch("COMMIT").unwrap();
        }
    }

    // Requirement: Hook identity and backup metadata exclude Hook payload
    // Scenario: Schema v8 upgrades atomically to v9
    #[test]
    fn test_v9_populated_v8_upgrades_without_data_loss() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v8(&conn);

        let untouched = [
            "skills",
            "skill_tags",
            "scenarios",
            "scenario_skills",
            "scenario_skill_tools",
            "active_scenario",
            "projects",
            "settings",
            "discovered_skills",
            "audit_log",
            "pending_conflicts",
            "artifacts",
            "artifact_deployments",
        ];
        let before: Vec<Vec<String>> = untouched.iter().map(|t| dump_table(&conn, t)).collect();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
        for (table, expected) in untouched.iter().zip(before.iter()) {
            assert_eq!(&dump_table(&conn, table), expected, "table {table} changed");
        }
        assert!(table_exists(&conn, "hook_details"));
        assert!(table_exists(&conn, "hook_backups"));
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM hook_details"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM hook_backups"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_v9_fresh_database_has_empty_hook_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM hook_details"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM hook_backups"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_v9_fresh_and_upgraded_schemas_match() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&fresh).unwrap();

        let upgraded = Connection::open_in_memory().unwrap();
        populated_v8(&upgraded);
        run_migrations(&upgraded).unwrap();

        assert_eq!(schema_snapshot(&fresh), schema_snapshot(&upgraded));
    }

    #[test]
    fn test_v9_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v8(&conn);
        run_migrations(&conn).unwrap();
        let schema = schema_snapshot(&conn);

        run_migrations(&conn).unwrap();

        assert_eq!(schema_snapshot(&conn), schema);
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    /// A half-applied v9 would leave one Hook table visible without the other,
    /// so the step has to be all-or-nothing.
    #[test]
    fn test_v9_migration_failure_rolls_back_to_v8() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v8(&conn);
        // A table already standing in the way makes the DDL step fail.
        conn.execute_batch("CREATE TABLE hook_details (blocker TEXT);")
            .unwrap();
        let schema_before = schema_snapshot(&conn);
        let artifacts_before = dump_table(&conn, "artifacts");

        run_migrations(&conn).unwrap_err();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 8);
        assert_eq!(schema_snapshot(&conn), schema_before);
        assert_eq!(dump_table(&conn, "artifacts"), artifacts_before);
        assert!(!table_exists(&conn, "hook_backups"));
    }

    // ── Schema v10: Config Profile typed persistence and recovery metadata ──

    /// A database that has been migrated up to v9 and stopped there, which is
    /// what an installed copy of the previous release holds.
    fn populated_v9(conn: &Connection) {
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_v7_schema(conn);
        populate_v7_fixture(conn);
        for version in 7..9 {
            conn.execute_batch("BEGIN EXCLUSIVE").unwrap();
            migrate_step(conn, version).unwrap();
            conn.pragma_update(None, "user_version", version + 1).unwrap();
            conn.execute_batch("COMMIT").unwrap();
        }
    }

    /// A `config_profile` Artifact plus its detail row, which every entry and
    /// recovery constraint test needs as a parent.
    fn seed_config_profile(conn: &Connection, artifact_id: &str, name: &str) {
        conn.execute(
            "INSERT INTO artifacts (id, kind) VALUES (?1, 'config_profile')",
            [artifact_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO config_profile_details (artifact_id, name, revision, created_at, updated_at)
             VALUES (?1, ?2, 1, 100, 100)",
            [artifact_id, name],
        )
        .unwrap();
    }

    // Requirement: Profiles persist only exact typed non-sensitive settings
    // Scenario: Schema v9 upgrades atomically to v10
    #[test]
    fn test_config_profile_management_migration_upgrades_v9_without_data_loss() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v9(&conn);

        let untouched = [
            "skills",
            "skill_tags",
            "scenarios",
            "scenario_skills",
            "scenario_skill_tools",
            "active_scenario",
            "projects",
            "settings",
            "discovered_skills",
            "audit_log",
            "pending_conflicts",
            "artifacts",
            "artifact_deployments",
            "hook_details",
            "hook_backups",
        ];
        let before: Vec<Vec<String>> = untouched.iter().map(|t| dump_table(&conn, t)).collect();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 10);
        for (table, expected) in untouched.iter().zip(before.iter()) {
            assert_eq!(&dump_table(&conn, table), expected, "table {table} changed");
        }
        assert!(table_exists(&conn, "config_profile_details"));
        assert!(table_exists(&conn, "config_profile_entries"));
        assert!(table_exists(&conn, "config_profile_recoveries"));
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_details"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_recoveries"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_config_profile_management_migration_fresh_database_has_empty_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        run_migrations(&conn).unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 10);
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_details"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_recoveries"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_config_profile_management_migration_fresh_and_upgraded_schemas_match() {
        let fresh = Connection::open_in_memory().unwrap();
        fresh.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&fresh).unwrap();

        let upgraded = Connection::open_in_memory().unwrap();
        populated_v9(&upgraded);
        run_migrations(&upgraded).unwrap();

        assert_eq!(schema_snapshot(&fresh), schema_snapshot(&upgraded));
    }

    #[test]
    fn test_config_profile_management_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v9(&conn);
        run_migrations(&conn).unwrap();
        let schema = schema_snapshot(&conn);

        run_migrations(&conn).unwrap();

        assert_eq!(schema_snapshot(&conn), schema);
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 10);
    }

    /// A half-applied v10 would leave the deployment table rebuilt without the
    /// profile tables that justify the new mode, so the step is all-or-nothing.
    #[test]
    fn test_config_profile_management_migration_failure_rolls_back_to_v9() {
        let conn = Connection::open_in_memory().unwrap();
        populated_v9(&conn);
        // A table already standing in the way makes the DDL step fail.
        conn.execute_batch("CREATE TABLE config_profile_entries (blocker TEXT);")
            .unwrap();
        let schema_before = schema_snapshot(&conn);
        let artifacts_before = dump_table(&conn, "artifacts");
        let deployments_before = dump_table(&conn, "artifact_deployments");

        run_migrations(&conn).unwrap_err();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 9);
        assert_eq!(schema_snapshot(&conn), schema_before);
        assert_eq!(dump_table(&conn, "artifacts"), artifacts_before);
        assert_eq!(dump_table(&conn, "artifact_deployments"), deployments_before);
        assert!(!table_exists(&conn, "config_profile_details"));
        assert!(!table_exists(&conn, "config_profile_recoveries"));
    }

    /// The detail table is the one-to-one subtype of a `config_profile`
    /// Artifact. A CHECK cannot reach the parent row, so the kind constraint is
    /// carried by a trigger the same way Hook details carry theirs.
    #[test]
    fn test_config_profile_management_migration_detail_requires_config_profile_kind() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        conn.execute("INSERT INTO artifacts (id, kind) VALUES ('a-hook', 'hook')", [])
            .unwrap();

        conn.execute(
            "INSERT INTO config_profile_details (artifact_id, name, revision, created_at, updated_at)
             VALUES ('a-hook', 'Wrong kind', 1, 100, 100)",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_details"),
            "Integer(0)"
        );
    }

    #[test]
    fn test_config_profile_management_migration_entry_requires_existing_detail() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO config_profile_entries
                (id, artifact_id, agent, canonical_key, value_type, string_value)
             VALUES ('e-1', 'missing-profile', 'codex', 'model', 'string', 'gpt-5')",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(0)"
        );
    }

    /// Deleting the profile Artifact must take its detail and entries with it,
    /// so no entry can outlive the profile that gave it meaning.
    #[test]
    fn test_config_profile_management_migration_entries_cascade_with_the_artifact() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        seed_config_profile(&conn, "prof-1", "Development");
        conn.execute(
            "INSERT INTO config_profile_entries
                (id, artifact_id, agent, canonical_key, value_type, string_value)
             VALUES ('e-1', 'prof-1', 'codex', 'sandbox_mode', 'string', 'read-only')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM artifacts WHERE id = 'prof-1'", [])
            .unwrap();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_details"),
            "Integer(0)"
        );
        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(0)"
        );
    }

    // Requirement: Profiles persist only exact typed non-sensitive settings
    // Scenario: Valid mixed-Agent profile is stored as typed entries
    #[test]
    fn test_config_profile_management_migration_entry_key_is_unique_per_agent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        seed_config_profile(&conn, "prof-1", "Development");

        // The same canonical key for two Agents is the documented mixed-Agent
        // profile, so it must be accepted.
        conn.execute(
            "INSERT INTO config_profile_entries
                (id, artifact_id, agent, canonical_key, value_type, string_value)
             VALUES ('e-1', 'prof-1', 'codex', 'model', 'string', 'gpt-5'),
                    ('e-2', 'prof-1', 'claude_code', 'model', 'string', 'sonnet')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO config_profile_entries
                (id, artifact_id, agent, canonical_key, value_type, string_value)
             VALUES ('e-3', 'prof-1', 'codex', 'model', 'string', 'gpt-5.1')",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(2)"
        );
    }

    /// The mutually exclusive scalar columns are what keep a raw document, a
    /// nested table or an untyped blob out of the row: exactly one typed column
    /// may hold a value, and it must be the one `value_type` names.
    #[test]
    fn test_config_profile_management_migration_entry_requires_exactly_one_typed_value() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        seed_config_profile(&conn, "prof-1", "Development");

        let accepted = [
            "('ok-1', 'prof-1', 'codex', 'sandbox_mode', 'string', 'read-only', NULL, NULL)",
            "('ok-2', 'prof-1', 'claude_code', 'always_thinking_enabled', 'boolean', NULL, 1, NULL)",
            "('ok-3', 'prof-1', 'claude_code', 'cleanup_period_days', 'integer', NULL, NULL, 20)",
        ];
        for values in accepted {
            conn.execute(
                &format!(
                    "INSERT INTO config_profile_entries
                        (id, artifact_id, agent, canonical_key, value_type,
                         string_value, boolean_value, integer_value)
                     VALUES {values}"
                ),
                [],
            )
            .unwrap_or_else(|err| panic!("{values} should be accepted: {err}"));
        }

        let rejected = [
            // Typed column missing for the declared type.
            "('no-1', 'prof-1', 'codex', 'model', 'string', NULL, NULL, NULL)",
            "('no-2', 'prof-1', 'codex', 'web_search', 'boolean', NULL, NULL, NULL)",
            "('no-3', 'prof-1', 'claude_code', 'fast_mode', 'integer', NULL, NULL, NULL)",
            // Value in a column the declared type does not own.
            "('no-4', 'prof-1', 'codex', 'model', 'string', 'gpt-5', 1, NULL)",
            "('no-5', 'prof-1', 'codex', 'web_search', 'boolean', 'yes', 1, NULL)",
            "('no-6', 'prof-1', 'claude_code', 'cleanup_period_days', 'integer', NULL, 1, 20)",
            // A type outside the scalar vocabulary.
            "('no-7', 'prof-1', 'codex', 'model', 'table', 'gpt-5', NULL, NULL)",
            // A boolean column that is not a boolean.
            "('no-8', 'prof-1', 'codex', 'web_search', 'boolean', NULL, 2, NULL)",
            // An Agent outside the two this capability writes.
            "('no-9', 'prof-1', 'gemini', 'model', 'string', 'gpt-5', NULL, NULL)",
        ];
        for values in rejected {
            conn.execute(
                &format!(
                    "INSERT INTO config_profile_entries
                        (id, artifact_id, agent, canonical_key, value_type,
                         string_value, boolean_value, integer_value)
                     VALUES {values}"
                ),
                [],
            )
            .expect_err(&format!("{values} should be rejected"));
        }

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_entries"),
            "Integer(3)"
        );
    }

    // Requirement: Restore is previewed and conflict-safe
    // Scenario: Existing source is restored after exact preview
    #[test]
    fn test_config_profile_management_migration_recovery_is_latest_per_assignment() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        seed_config_profile(&conn, "prof-1", "Development");

        conn.execute(
            "INSERT INTO config_profile_recoveries
                (id, artifact_id, project_id, agent, source_id, kind, before_hash, after_hash,
                 locator, revision, created_at)
             VALUES ('r-1', 'prof-1', 'proj-1', 'codex', 'codex:project:config-toml', 'bytes',
                     'aaa', 'bbb', 'prof-1/proj-1/codex/latest', 1, 100)",
            [],
        )
        .unwrap();

        // Only the latest recovery point per profile, Project and Agent is ever
        // exposed, so a second row for the same assignment is a schema error.
        conn.execute(
            "INSERT INTO config_profile_recoveries
                (id, artifact_id, project_id, agent, source_id, kind, before_hash, after_hash,
                 locator, revision, created_at)
             VALUES ('r-2', 'prof-1', 'proj-1', 'codex', 'codex:project:config-toml', 'absent',
                     'bbb', 'ccc', '', 2, 200)",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_recoveries"),
            "Integer(1)"
        );
    }

    #[test]
    fn test_config_profile_management_migration_recovery_requires_config_profile_kind() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        conn.execute("INSERT INTO artifacts (id, kind) VALUES ('a-hook', 'hook')", [])
            .unwrap();

        conn.execute(
            "INSERT INTO config_profile_recoveries
                (id, artifact_id, project_id, agent, source_id, kind, before_hash, after_hash,
                 locator, revision, created_at)
             VALUES ('r-1', 'a-hook', 'proj-1', 'codex', 'codex:project:config-toml', 'bytes',
                     'aaa', 'bbb', 'a-hook/proj-1/codex/latest', 1, 100)",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM config_profile_recoveries"),
            "Integer(0)"
        );
    }

    /// Assignments reuse canonical deployments, which means the deployment mode
    /// vocabulary has to admit the Config Profile managed write. The rebuild
    /// that widens the CHECK must carry every existing row across unchanged.
    #[test]
    fn test_config_profile_management_migration_deployments_accept_config_profile_mode() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();
        seed_config_profile(&conn, "prof-1", "Development");

        conn.execute(
            "INSERT INTO artifact_deployments
                (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
                 target_path, last_synced_hash, last_synced_at, status, last_error)
             VALUES ('d-1', 'prof-1', 'project', 'proj-1', 'codex', 1, 'config-profile', '', '',
                     NULL, NULL, 'pending', NULL)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO artifact_deployments
                (id, artifact_id, scope_type, scope_id, agent, enabled, mode, source_path,
                 target_path, last_synced_hash, last_synced_at, status, last_error)
             VALUES ('d-2', 'prof-1', 'project', 'proj-1', 'claude_code', 1, 'raw-write', '', '',
                     NULL, NULL, 'pending', NULL)",
            [],
        )
        .unwrap_err();

        assert_eq!(
            scalar(&conn, "SELECT COUNT(*) FROM artifact_deployments"),
            "Integer(1)"
        );
    }
}
