use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use super::central_repo;
use super::repo_lock::RepoLock;
use super::skill_metadata;
use super::skill_store::{ScenarioRecord, SkillRecord, SkillStore};

const SCHEMA_VERSION: u32 = 1;
const APP_MIN_VERSION: &str = "2.0.0";

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaFile {
    pub schema_version: u32,
    pub app_min_version: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMeta {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub subpath: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMetaFile {
    pub schema_version: u32,
    pub skill_id: String,
    pub path: String,
    pub path_key: String,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub source: SourceMeta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioMetaFile {
    pub schema_version: u32,
    pub scenario_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioSkillMetaFile {
    pub schema_version: u32,
    pub scenario_id: String,
    pub skill_id: String,
    pub sort_order: i32,
    pub tools: BTreeMap<String, bool>,
}

pub fn metadata_dir() -> PathBuf {
    central_repo::skills_dir().join(".skills-manager")
}

pub fn metadata_exists() -> bool {
    metadata_dir().join("schema.json").exists()
        || metadata_dir().join("skills").is_dir()
        || metadata_dir().join("scenarios").is_dir()
}

pub fn has_complete_skill_snapshot() -> bool {
    metadata_dir().join("schema.json").is_file() && metadata_dir().join("skills").is_dir()
}

#[allow(dead_code)]
pub fn write_all_from_db(store: &SkillStore) -> Result<()> {
    // Foreground wait: this runs at startup and from CLI preset/enable
    // commands, which must succeed (startup aborts on error), not skip.
    let _lock = RepoLock::acquire_foreground("write sync metadata")?;
    write_all_from_db_unlocked(store)
}

/// Runs `f` while holding the central-repo lock. Callers are user-initiated
/// operations (set tags, delete, preset edits, imports), so we wait out
/// transient contention with background work instead of failing fast.
pub(crate) fn with_repo_lock<T, F>(operation: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let _lock = RepoLock::acquire_foreground(operation)?;
    f()
}

/// Same as [`with_repo_lock`], but refuses while the Library is offline and
/// keeps the `library_offline` error kind intact for the frontend. Every
/// user-initiated flow that writes Library files or deployment targets uses
/// this; `with_repo_lock` remains for state that lives on internal storage.
pub(crate) fn with_library_write_lock<T, F>(
    operation: &str,
    f: F,
) -> std::result::Result<T, crate::core::error::AppError>
where
    F: FnOnce() -> Result<T>,
{
    let _lock = RepoLock::acquire_library_write(operation)?;
    let outcome = f();
    if outcome.is_err() {
        // The pre-write check passed, so the volume was there when this started
        // and may have gone away mid-write. Re-probe rather than assume: a
        // Library that really is gone must not stay `online` until the next
        // page load, while an unrelated failure (permissions, disk full) must
        // not be reported to the user as a disconnected Library.
        crate::core::library_availability::poll_availability();
    }
    outcome.map_err(crate::core::error::AppError::db)
}

pub(crate) fn write_all_from_db_unlocked(store: &SkillStore) -> Result<()> {
    ensure_metadata_dirs()?;
    write_schema()?;
    write_skill_records_from_db(store)?;
    write_scenario_records_from_db(store)?;
    remove_stale_metadata_files(store)?;
    Ok(())
}

#[allow(dead_code)]
pub fn reindex_from_metadata(store: &SkillStore) -> Result<()> {
    // Foreground wait: runs at process startup (GUI and every CLI invocation),
    // which aborts on error — better to wait out transient contention than fail.
    let _lock = RepoLock::acquire_foreground("reindex sync metadata")?;
    reindex_from_metadata_unlocked(store)
}

pub(crate) fn reindex_from_metadata_unlocked(store: &SkillStore) -> Result<()> {
    if !metadata_exists() {
        return Ok(());
    }
    if !has_complete_skill_snapshot() {
        bail!("incomplete sync metadata snapshot: missing schema.json or skills directory");
    }

    let skills = read_skill_files()?;
    if skills.is_empty() && central_repo_has_valid_skill_dirs()? {
        bail!(
            "sync metadata contains no skills, but the central repository contains skill directories"
        );
    }
    ensure_unique_path_keys(&skills)?;

    let has_complete_scenario_snapshot = metadata_has_complete_scenario_snapshot();
    let scenarios = if has_complete_scenario_snapshot {
        read_scenario_files()?
    } else {
        Vec::new()
    };
    let memberships = if has_complete_scenario_snapshot {
        read_membership_files()?
    } else {
        Vec::new()
    };
    let now = chrono::Utc::now().timestamp_millis();
    let skills_root = central_repo::skills_dir();

    let metadata_ids: HashSet<String> = skills.iter().map(|m| m.skill_id.clone()).collect();
    for existing in store.get_all_skills()? {
        if !metadata_ids.contains(&existing.id) {
            store.delete_skill(&existing.id)?;
        }
    }

    let existing_by_id: HashMap<String, SkillRecord> = store
        .get_all_skills()?
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect();

    // Every surviving row gets its central_path rewritten below; park them
    // on placeholders first so path reassignments between skills (renames or
    // merge collision reshuffles) cannot trip the UNIQUE constraint mid-loop.
    store.park_central_paths_for_reindex()?;

    for meta in skills {
        let skill_dir = skills_root.join(&meta.path);
        if !skill_dir.is_dir() {
            store.delete_skill(&meta.skill_id)?;
            continue;
        }

        let parsed = skill_metadata::parse_skill_md(&skill_dir);
        let inferred_name = skill_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown-skill".to_string());
        let name = parsed
            .name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(inferred_name);
        let previous = existing_by_id.get(&meta.skill_id);
        let source_ref = if matches!(meta.source.source_type.as_str(), "import" | "local") {
            previous.and_then(|s| s.source_ref.clone())
        } else {
            meta.source.ref_.clone()
        };
        let central_path = skill_dir.to_string_lossy().to_string();

        let new_hash = super::content_hash::hash_directory(&skill_dir).ok();
        let updated_at = match previous {
            Some(prev) if prev.content_hash == new_hash => prev.updated_at,
            _ => now,
        };

        let record = SkillRecord {
            id: meta.skill_id.clone(),
            name,
            description: parsed.description,
            source_type: meta.source.source_type.clone(),
            source_ref,
            source_ref_resolved: previous.and_then(|s| s.source_ref_resolved.clone()),
            source_subpath: meta.source.subpath.clone(),
            source_branch: meta.source.branch.clone(),
            source_revision: previous.and_then(|s| s.source_revision.clone()),
            remote_revision: previous.and_then(|s| s.remote_revision.clone()),
            central_path,
            content_hash: new_hash,
            enabled: meta.enabled,
            created_at: previous.map(|s| s.created_at).unwrap_or(now),
            updated_at,
            status: "ok".to_string(),
            update_status: previous
                .map(|s| s.update_status.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            last_checked_at: previous.and_then(|s| s.last_checked_at),
            last_check_error: previous.and_then(|s| s.last_check_error.clone()),
        };
        store.upsert_skill(&record)?;
        store.set_tags_for_skill(&meta.skill_id, &meta.tags)?;
    }

    if has_complete_scenario_snapshot {
        store.replace_scenarios_from_metadata(&scenarios)?;
        store.replace_scenario_memberships_from_metadata(&memberships)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn ensure_skill_metadata(store: &SkillStore, skill_id: &str) -> Result<()> {
    // Foreground wait: the CLI `tag` commands flush metadata here after a DB
    // write; they should ride out contention rather than fail fast.
    let _lock = RepoLock::acquire_foreground("write skill metadata")?;
    ensure_skill_metadata_unlocked(store, skill_id)
}

pub(crate) fn ensure_skill_metadata_unlocked(store: &SkillStore, skill_id: &str) -> Result<()> {
    write_schema()?;
    let skill = store
        .get_skill_by_id(skill_id)?
        .ok_or_else(|| anyhow!("skill not found: {skill_id}"))?;
    let tags = store.get_tags_map()?.remove(skill_id).unwrap_or_default();
    write_skill_file(&skill, &tags)
}

pub fn cleanup_temporary_files() -> Result<()> {
    let root = metadata_dir();
    if !root.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name.contains(".tmp.") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

fn write_schema() -> Result<()> {
    let schema = SchemaFile {
        schema_version: SCHEMA_VERSION,
        app_min_version: APP_MIN_VERSION.to_string(),
        created_by: "skills-manager".to_string(),
    };
    atomic_write_json(&metadata_dir().join("schema.json"), &schema)
}

fn ensure_metadata_dirs() -> Result<()> {
    fs::create_dir_all(metadata_dir().join("skills"))?;
    fs::create_dir_all(metadata_dir().join("scenarios"))?;
    fs::create_dir_all(metadata_dir().join("scenario-skills"))?;
    Ok(())
}

fn metadata_has_complete_scenario_snapshot() -> bool {
    metadata_dir().join("schema.json").is_file()
        && metadata_dir().join("scenarios").is_dir()
        && metadata_dir().join("scenario-skills").is_dir()
}

fn write_skill_records_from_db(store: &SkillStore) -> Result<()> {
    let mut tags = store.get_tags_map()?;
    for skill in store.get_all_skills()? {
        write_skill_file(&skill, &tags.remove(&skill.id).unwrap_or_default())?;
    }
    Ok(())
}

fn write_scenario_records_from_db(store: &SkillStore) -> Result<()> {
    for scenario in store.get_all_scenarios()? {
        write_scenario_file(&scenario)?;
        let skill_ids = store.get_skill_ids_for_scenario(&scenario.id)?;
        for (index, skill_id) in skill_ids.iter().enumerate() {
            let tools = store
                .get_scenario_skill_tool_toggles(&scenario.id, skill_id)?
                .into_iter()
                .map(|toggle| (toggle.tool, toggle.enabled))
                .collect::<BTreeMap<_, _>>();
            let member = ScenarioSkillMetaFile {
                schema_version: SCHEMA_VERSION,
                scenario_id: scenario.id.clone(),
                skill_id: skill_id.clone(),
                sort_order: index as i32,
                tools,
            };
            write_membership_file(&member)?;
        }
    }
    Ok(())
}

fn remove_stale_metadata_files(store: &SkillStore) -> Result<()> {
    let skill_ids: HashSet<String> = store
        .get_all_skills()?
        .into_iter()
        .map(|skill| skill.id)
        .collect();
    remove_stale_json_files(&metadata_dir().join("skills"), &skill_ids)?;

    let scenario_ids: HashSet<String> = store
        .get_all_scenarios()?
        .into_iter()
        .map(|scenario| scenario.id)
        .collect();
    remove_stale_json_files(&metadata_dir().join("scenarios"), &scenario_ids)?;

    let membership_root = metadata_dir().join("scenario-skills");
    if membership_root.exists() {
        let mut expected_memberships = HashSet::new();
        for scenario_id in &scenario_ids {
            for skill_id in store.get_skill_ids_for_scenario(scenario_id)? {
                expected_memberships.insert((scenario_id.clone(), skill_id));
            }
        }

        for entry in fs::read_dir(&membership_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let scenario_id = entry.file_name().to_string_lossy().to_string();
            if !scenario_ids.contains(&scenario_id) {
                fs::remove_dir_all(entry.path())?;
                continue;
            }

            let mut has_files = false;
            for member in fs::read_dir(entry.path())? {
                let member = member?;
                if !member.file_type()?.is_file() {
                    continue;
                }
                let path = member.path();
                if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                    let skill_id = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !expected_memberships.contains(&(scenario_id.clone(), skill_id)) {
                        fs::remove_file(&path)?;
                    } else {
                        has_files = true;
                    }
                }
            }
            if !has_files && fs::read_dir(entry.path())?.next().is_none() {
                fs::remove_dir(entry.path())?;
            }
        }
    }

    Ok(())
}

fn remove_stale_json_files(dir: &Path, expected_stems: &HashSet<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().map(|ext| ext == "json").unwrap_or(false) {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            if !expected_stems.contains(&stem) {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

fn write_skill_file(skill: &SkillRecord, tags: &[String]) -> Result<()> {
    let path = relative_skill_path(&skill.central_path)?;
    let tags = sorted_tags(tags);
    let source_ref = match skill.source_type.as_str() {
        "import" | "local" => None,
        _ => skill.source_ref.clone(),
    };
    let meta = SkillMetaFile {
        schema_version: SCHEMA_VERSION,
        skill_id: skill.id.clone(),
        path_key: path_key(&path),
        path,
        enabled: skill.enabled,
        tags,
        source: SourceMeta {
            source_type: skill.source_type.clone(),
            ref_: source_ref,
            subpath: skill.source_subpath.clone(),
            branch: skill.source_branch.clone(),
        },
    };
    atomic_write_json(
        &metadata_dir()
            .join("skills")
            .join(format!("{}.json", skill.id)),
        &meta,
    )
}

fn write_scenario_file(scenario: &ScenarioRecord) -> Result<()> {
    let meta = ScenarioMetaFile {
        schema_version: SCHEMA_VERSION,
        scenario_id: scenario.id.clone(),
        name: scenario.name.clone(),
        description: scenario.description.clone(),
        icon: scenario.icon.clone(),
        sort_order: scenario.sort_order,
    };
    atomic_write_json(
        &metadata_dir()
            .join("scenarios")
            .join(format!("{}.json", scenario.id)),
        &meta,
    )
}

fn write_membership_file(member: &ScenarioSkillMetaFile) -> Result<()> {
    atomic_write_json(
        &metadata_dir()
            .join("scenario-skills")
            .join(&member.scenario_id)
            .join(format!("{}.json", member.skill_id)),
        member,
    )
}

fn read_skill_files() -> Result<Vec<SkillMetaFile>> {
    read_json_files(metadata_dir().join("skills"))
}

fn central_repo_has_valid_skill_dirs() -> Result<bool> {
    let skills_dir = central_repo::skills_dir();
    if !skills_dir.exists() {
        return Ok(false);
    }

    for entry in WalkDir::new(&skills_dir)
        .min_depth(1)
        .max_depth(6)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != ".git" && name != ".skills-manager"
        })
    {
        let entry = entry?;
        if entry.file_type().is_dir() && skill_metadata::is_valid_skill_dir(entry.path()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn read_scenario_files() -> Result<Vec<ScenarioMetaFile>> {
    read_json_files(metadata_dir().join("scenarios"))
}

fn read_membership_files() -> Result<Vec<ScenarioSkillMetaFile>> {
    let root = metadata_dir().join("scenario-skills");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "json")
        {
            let raw = fs::read_to_string(entry.path())?;
            records.push(serde_json::from_str(&raw).with_context(|| {
                format!(
                    "invalid scenario membership metadata {}",
                    entry.path().display()
                )
            })?);
        }
    }
    Ok(records)
}

fn read_json_files<T: for<'de> Deserialize<'de>>(dir: PathBuf) -> Result<Vec<T>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let is_json = entry
            .path()
            .extension()
            .map(|ext| ext == "json")
            .unwrap_or(false);
        if !entry.file_type()?.is_file() || !is_json {
            continue;
        }
        let raw = fs::read_to_string(entry.path())?;
        records.push(
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid metadata file {}", entry.path().display()))?,
        );
    }
    Ok(records)
}

fn ensure_unique_path_keys(skills: &[SkillMetaFile]) -> Result<()> {
    let mut seen = HashMap::new();
    for skill in skills {
        let computed = path_key(&skill.path);
        if computed != skill.path_key {
            bail!("path_key mismatch for skill {}", skill.skill_id);
        }
        if let Some(previous) = seen.insert(skill.path_key.clone(), skill.skill_id.clone()) {
            bail!(
                "case-insensitive path collision between {} and {}",
                previous,
                skill.skill_id
            );
        }
    }
    Ok(())
}

fn relative_skill_path(central_path: &str) -> Result<String> {
    let root = central_repo::skills_dir();
    let path = PathBuf::from(central_path);
    let relative = path
        .strip_prefix(&root)
        .with_context(|| format!("skill path is outside skills root: {}", path.display()))?;
    let value = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    if value.is_empty() || value.starts_with("../") {
        bail!("invalid relative skill path: {value}");
    }
    Ok(value)
}

/// Case/Unicode-folded key of a relative skill path. Shared with the merge
/// engine (§2.1): metadata rebuilt during a merge must use the exact same
/// folding as metadata written from the DB.
pub(crate) fn path_key(path: &str) -> String {
    path.split('/')
        .map(|part| caseless::default_case_fold_str(&part.nfc().collect::<String>()))
        .collect::<Vec<_>>()
        .join("/")
}

fn sorted_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Canonical on-disk JSON encoding for metadata files. The merge engine
/// reuses this to rebuild merged metadata blobs so that two devices merging
/// the same pair of commits produce byte-identical blobs → identical tree
/// OIDs (§2.1 / §10).
pub(crate) fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = canonical_json_bytes(value)?;
    let tmp = path.with_extension(format!("json.tmp.{}", uuid::Uuid::now_v7()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_parent_dir(path)?;
    Ok(())
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        // Windows requires FILE_FLAG_BACKUP_SEMANTICS to open a directory
        // handle. std::fs::File::open uses ordinary file semantics and fails
        // with Access is denied (os error 5) for directories.
        // FlushFileBuffers also requires a handle opened with write access.
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

        let dir = fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(parent)?;
        dir.sync_all()?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let dir = fs::File::open(parent)?;
        dir.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{central_repo, skill_store::SkillStore};
    use std::sync::MutexGuard;
    use tempfile::{TempDir, tempdir};

    struct TestRepo {
        _lock: MutexGuard<'static, ()>,
        _tmp: TempDir,
        store: SkillStore,
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            central_repo::set_test_base_dir_override(None);
        }
    }

    fn test_repo() -> TestRepo {
        let lock = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        let base = tmp.path().join("repo");
        central_repo::set_test_base_dir_override(Some(base.clone()));
        fs::create_dir_all(central_repo::skills_dir()).unwrap();
        let store = SkillStore::new(&base.join("test.db")).unwrap();
        TestRepo {
            _lock: lock,
            _tmp: tmp,
            store,
        }
    }

    /// Same as [`test_repo`], but the database file is seeded at schema v7 and
    /// only then opened through `SkillStore`, which runs the Artifact
    /// migration. What comes back is an upgraded database, not a fresh one.
    fn test_repo_upgraded_from_v7() -> TestRepo {
        let lock = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        let base = tmp.path().join("repo");
        central_repo::set_test_base_dir_override(Some(base.clone()));
        fs::create_dir_all(central_repo::skills_dir()).unwrap();

        let db_path = base.join("test.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            crate::core::migrations::create_v7_schema(&conn).unwrap();
            seed_backup_dataset_v7(&conn, &demo_central_path());
        }

        let store = SkillStore::new(&db_path).unwrap();
        TestRepo {
            _lock: lock,
            _tmp: tmp,
            store,
        }
    }

    /// The dataset behind the backup round-trip: one Skill with Tags, one
    /// Scenario, one membership with tool toggles, plus a legacy target so the
    /// upgraded database really did carry deployment data.
    fn demo_central_path() -> String {
        central_repo::skills_dir()
            .join("demo")
            .to_string_lossy()
            .into_owned()
    }

    fn seed_backup_dataset_v7(conn: &rusqlite::Connection, central_path: &str) {
        conn.execute(
            "INSERT INTO skills (id, name, description, source_type, source_ref, central_path,
                                 content_hash, enabled, created_at, updated_at, status, update_status)
             VALUES ('skill-1', 'demo', 'a demo skill', 'local', NULL, ?1, 'hash1', 1, 10, 11,
                     'ok', 'local_only')",
            [central_path],
        )
        .unwrap();
        conn.execute_batch(
            "
            INSERT INTO skill_tags (skill_id, tag) VALUES ('skill-1', 'alpha'), ('skill-1', 'beta');
            INSERT INTO skill_targets (id, skill_id, tool, target_path, mode, status, synced_at, source_hash)
            VALUES ('target-1', 'skill-1', 'codex', '/tmp/project/.agents/skills/demo', 'symlink', 'ok', 1000, 'hash1');
            INSERT INTO scenarios (id, name, description, icon, sort_order, created_at, updated_at)
            VALUES ('sc-1', 'Work', 'work setup', 'briefcase', 0, 20, 21);
            INSERT INTO scenario_skills (scenario_id, skill_id, added_at, sort_order)
            VALUES ('sc-1', 'skill-1', 30, 0);
            INSERT INTO scenario_skill_tools (scenario_id, skill_id, tool, enabled, updated_at)
            VALUES ('sc-1', 'skill-1', 'codex', 1, 31), ('sc-1', 'skill-1', 'claude', 0, 32);
            ",
        )
        .unwrap();
    }

    /// The identical dataset written through the store API onto a fresh v8
    /// database.
    fn seed_backup_dataset_v8(store: &SkillStore) {
        store
            .insert_skill(&SkillRecord {
                id: "skill-1".to_string(),
                name: "demo".to_string(),
                description: Some("a demo skill".to_string()),
                source_type: "local".to_string(),
                source_ref: None,
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: demo_central_path(),
                content_hash: Some("hash1".to_string()),
                enabled: true,
                created_at: 10,
                updated_at: 11,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: None,
                last_check_error: None,
            })
            .unwrap();
        store
            .set_tags_for_skill("skill-1", &["alpha".into(), "beta".into()])
            .unwrap();
        store
            .insert_scenario(&ScenarioRecord {
                id: "sc-1".to_string(),
                name: "Work".to_string(),
                description: Some("work setup".to_string()),
                icon: Some("briefcase".to_string()),
                sort_order: 0,
                created_at: 20,
                updated_at: 21,
            })
            .unwrap();
        store.add_skill_to_scenario("sc-1", "skill-1").unwrap();
        store
            .set_scenario_skill_tool_enabled("sc-1", "skill-1", "codex", true)
            .unwrap();
        store
            .set_scenario_skill_tool_enabled("sc-1", "skill-1", "claude", false)
            .unwrap();
    }

    /// Every file under `.skills-manager`, as (relative path, bytes).
    fn metadata_snapshot() -> Vec<(String, Vec<u8>)> {
        let root = metadata_dir();
        let mut files: Vec<(String, Vec<u8>)> = WalkDir::new(&root)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                let rel = entry
                    .path()
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, fs::read(entry.path()).unwrap())
            })
            .collect();
        files.sort();
        files
    }

    /// The Artifact migration changes where deployments live, not what a backup
    /// contains. Metadata is written from the same dataset on a fresh v8
    /// database and on one upgraded from v7; the bytes must match exactly, and
    /// the upgrade must not introduce a metadata directory an older client
    /// would not understand.
    #[test]
    fn skill_metadata_bytes_survive_the_artifact_migration_unchanged() {
        let fresh_bytes = {
            let repo = test_repo();
            seed_backup_dataset_v8(&repo.store);
            write_all_from_db_unlocked(&repo.store).unwrap();
            metadata_snapshot()
        };

        let upgraded_bytes = {
            let repo = test_repo_upgraded_from_v7();
            // The upgrade really happened: the legacy target became a canonical
            // deployment and the Skill gained its Artifact identity.
            assert_eq!(repo.store.get_all_targets().unwrap().len(), 1);
            assert_eq!(
                repo.store.get_artifact("skill-1").unwrap().unwrap().kind,
                crate::core::artifact::ArtifactKind::Skill
            );
            write_all_from_db_unlocked(&repo.store).unwrap();
            metadata_snapshot()
        };

        assert_eq!(upgraded_bytes, fresh_bytes);

        let names: Vec<&str> = upgraded_bytes.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "scenario-skills/sc-1/skill-1.json",
                "scenarios/sc-1.json",
                "schema.json",
                "skills/skill-1.json",
            ],
            "the upgrade must not add Artifact or deployment metadata files"
        );

        // The canonical format itself is pinned, so a change in serialization
        // fails here rather than silently breaking older clients.
        let schema = String::from_utf8(upgraded_bytes[2].1.clone()).unwrap();
        assert!(schema.contains("\"schema_version\": 1"), "{schema}");
        assert_eq!(SCHEMA_VERSION, 1);
        assert_eq!(crate::core::merge::protocol::MERGE_PROTOCOL_VERSION, 2);

        let skill_meta = String::from_utf8(upgraded_bytes[3].1.clone()).unwrap();
        let parsed: SkillMetaFile = serde_json::from_str(&skill_meta).unwrap();
        assert_eq!(parsed.skill_id, "skill-1");
        assert_eq!(parsed.tags, vec!["alpha".to_string(), "beta".to_string()]);
        // serde_json orders object keys alphabetically when parsing, so this
        // pins the field set rather than the on-disk order.
        let keys: Vec<String> = serde_json::from_str::<serde_json::Value>(&skill_meta)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            keys,
            vec![
                "enabled",
                "path",
                "path_key",
                "schema_version",
                "skill_id",
                "source",
                "tags",
            ]
        );
    }

    /// Preset edits, scan imports, tag writes, and tool toggles all run their
    /// work inside this lock, so the gate has to refuse before the closure runs
    /// — Library metadata lives inside `skills_dir()`.
    #[test]
    fn library_write_lock_does_not_run_its_work_while_offline() {
        use crate::core::library_availability::{
            LibraryAvailability, LibraryReason, LibraryState, set_availability,
        };

        let repo = test_repo();
        let base = central_repo::skills_dir();
        set_availability(LibraryAvailability {
            state: LibraryState::Offline,
            reason: LibraryReason::NotWritable,
            configured_path: base,
            library_id: None,
        });

        let mut ran = false;
        let refused = with_library_write_lock("create scenario", || {
            ran = true;
            write_all_from_db_unlocked(&repo.store)
        });

        let err = refused.expect_err("offline must fail closed");
        assert_eq!(err.kind, crate::core::error::ErrorKind::LibraryOffline);
        assert_eq!(err.message, "not_writable");
        assert!(!ran, "the guarded work must not run at all");
        assert!(
            !metadata_dir().exists(),
            "no metadata may be written into an offline Library"
        );
    }

    /// The pre-write check cannot cover a volume pulled *during* the write.
    /// Without the re-probe on failure the state stays `online`, so the banner
    /// never appears and every control stays enabled until the next page load.
    #[test]
    fn a_write_that_fails_on_a_vanished_library_transitions_to_offline() {
        use crate::core::library_availability::{
            LibraryAvailability, LibraryReason, LibraryState, availability, set_availability,
        };

        let _repo = test_repo();
        let base = central_repo::library_base_dir();
        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: base.clone(),
            library_id: None,
        });

        // The volume has to survive the gate and disappear *inside* the write —
        // removing it any earlier means `acquire_library_write` refuses and the
        // failure path under test never runs.
        let failed = with_library_write_lock("write metadata", || -> Result<()> {
            fs::remove_dir_all(&base).unwrap();
            Err(anyhow::anyhow!("write failed: the volume went away"))
        });

        let after = availability();
        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: base,
            library_id: None,
        });

        assert!(failed.is_err(), "the caller still sees its own failure");
        assert_eq!(after.state, LibraryState::Offline);
        assert_eq!(after.reason, LibraryReason::MissingPath);
    }

    /// A failure that is not the Library going away must not be reported as a
    /// disconnected Library — the user would go looking for a cable.
    #[test]
    fn a_write_that_fails_for_another_reason_stays_online() {
        use crate::core::library_availability::{
            LibraryAvailability, LibraryReason, LibraryState, availability, set_availability,
        };

        let _repo = test_repo();
        let base = central_repo::library_base_dir();
        set_availability(LibraryAvailability {
            state: LibraryState::Online,
            reason: LibraryReason::Ok,
            configured_path: base,
            library_id: None,
        });

        let failed = with_library_write_lock("write metadata", || -> Result<()> {
            Err(anyhow::anyhow!("disk full"))
        });

        let after = availability();

        assert!(failed.is_err());
        assert!(
            after.is_online(),
            "the Library is still there; only the write failed: {after:?}"
        );
    }

    fn write_skill_dir(name: &str) -> PathBuf {
        let dir = central_repo::skills_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
        dir
    }

    fn sample_skill(id: &str, central_path: &Path) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: Some(central_path.to_string_lossy().to_string()),
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: central_path.to_string_lossy().to_string(),
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
    fn incomplete_metadata_snapshot_does_not_delete_existing_skills() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();

        fs::create_dir_all(metadata_dir()).unwrap();
        fs::write(metadata_dir().join("schema.json"), "{}").unwrap();

        let err = reindex_from_metadata_unlocked(&repo.store).unwrap_err();
        assert!(err.to_string().contains("incomplete sync metadata"));
        assert!(repo.store.get_skill_by_id("skill-1").unwrap().is_some());
    }

    #[test]
    fn empty_skill_metadata_does_not_delete_central_skills() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();

        fs::create_dir_all(metadata_dir().join("skills")).unwrap();
        fs::write(metadata_dir().join("schema.json"), "{}").unwrap();

        let err = reindex_from_metadata_unlocked(&repo.store).unwrap_err();
        assert!(err.to_string().contains("contains no skills"));
        assert!(repo.store.get_skill_by_id("skill-1").unwrap().is_some());
    }

    #[test]
    fn metadata_reindex_preserves_skill_id_and_tags() {
        let source = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        source
            .store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();
        source
            .store
            .set_tags_for_skill("skill-1", &["tag-b".to_string(), "tag-a".to_string()])
            .unwrap();
        write_all_from_db_unlocked(&source.store).unwrap();

        let restored_store =
            SkillStore::new(&central_repo::base_dir().join("restored.db")).unwrap();
        reindex_from_metadata_unlocked(&restored_store).unwrap();

        let restored = restored_store
            .get_skill_by_id("skill-1")
            .unwrap()
            .expect("skill should be restored from metadata");
        assert_eq!(restored.central_path, skill_dir.to_string_lossy());
        assert_eq!(
            restored_store
                .get_tags_map()
                .unwrap()
                .remove("skill-1")
                .unwrap(),
            vec!["tag-a".to_string(), "tag-b".to_string()]
        );
    }

    #[test]
    fn reindex_preserves_updated_at_when_content_unchanged() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("stable-skill");

        let mut record = sample_skill("skill-stable", &skill_dir);
        record.updated_at = 1_000_000;
        record.content_hash =
            Some(super::super::content_hash::hash_directory(&skill_dir).unwrap());
        repo.store.insert_skill(&record).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        let after = repo
            .store
            .get_skill_by_id("skill-stable")
            .unwrap()
            .expect("skill must still exist");
        assert_eq!(
            after.updated_at, 1_000_000,
            "updated_at must not change when skill content is unchanged"
        );
    }

    #[test]
    fn reindex_updates_updated_at_when_content_changes() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("changing-skill");

        let mut record = sample_skill("skill-changing", &skill_dir);
        record.updated_at = 1_000_000;
        record.content_hash = Some("old-hash-before-change".to_string());
        repo.store.insert_skill(&record).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        // Modify the skill content so the hash changes
        fs::write(skill_dir.join("extra.md"), "new content").unwrap();

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        let after = repo
            .store
            .get_skill_by_id("skill-changing")
            .unwrap()
            .expect("skill must still exist");
        assert_ne!(
            after.updated_at, 1_000_000,
            "updated_at must be refreshed when skill content changes"
        );
    }
}
