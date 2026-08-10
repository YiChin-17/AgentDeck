use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::commands::projects::{
    classify_sync_status, ensure_dir_within_root, source_ref_matches_skill_path,
    ProjectSkillDocumentDto,
};
use crate::core::skill_store::{SkillRecord, SkillStore, SkillTargetRecord};
use crate::core::{
    content_hash, error::AppError, installer, library_availability, project_scanner,
    scenario_service, sync_engine, tool_adapters, tool_service,
};

fn target_path_equals_skill(target_path: &str, skill_path: &str) -> bool {
    if target_path == skill_path {
        return true;
    }
    match (
        std::fs::canonicalize(target_path),
        std::fs::canonicalize(skill_path),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn adapter_for_agent(
    store: &SkillStore,
    agent: &str,
) -> Result<tool_adapters::ToolAdapter, AppError> {
    tool_adapters::all_tool_adapters(store)
        .into_iter()
        .find(|adapter| adapter.key == agent)
        .ok_or_else(|| AppError::not_found(format!("Unknown agent: {}", agent)))
}

/// Skills living in the adapter's single writable root. Used by flows that may
/// write back to what they find (the startup target backfill), which must never
/// reach a discovery-only root — see [`scan_agent_local_skills`] for the read
/// path that also covers those.
fn read_agent_primary_skills(
    adapter: &tool_adapters::ToolAdapter,
) -> Vec<project_scanner::ProjectSkillInfo> {
    project_scanner::read_linked_workspace_skills(
        &adapter.skills_dir(),
        None,
        &adapter.key,
        &adapter.display_name,
        adapter.recursive_scan,
    )
}

/// One Agent Skills row: the existing skill fields plus the role of the root it
/// was found in. `read_only` marks a discovery-only source (e.g. Codex's legacy
/// `~/.codex/skills`), which may be read and imported but never written back to.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLocalSkillDto {
    #[serde(flatten)]
    pub skill: project_scanner::ProjectSkillInfo,
    pub read_only: bool,
}

/// A global Skill root for one agent, in precedence order.
struct AgentSkillRoot {
    path: PathBuf,
    /// Discovery-only root: listed, never written to.
    read_only: bool,
    recursive: bool,
}

/// A scan hit. Carries the root it came from so later actions bound the path
/// against the root that actually produced it, instead of re-deriving the
/// primary root and rejecting (or worse, mis-accepting) a legacy path.
struct ScannedAgentSkill {
    skill: project_scanner::ProjectSkillInfo,
    root: PathBuf,
    read_only: bool,
}

fn canonical_root(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The agent's global Skill roots: writable primary first, then every existing
/// discovery-only root. An adapter's roots can alias one another — an override
/// pointing at the legacy directory, or a symlink between them — so roots are
/// deduplicated by canonical path and the first one in precedence order wins.
/// That is what makes an override onto the legacy directory writable primary
/// rather than a second, read-only listing of the same files.
fn agent_skill_roots(adapter: &tool_adapters::ToolAdapter) -> Vec<AgentSkillRoot> {
    let mut roots = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    let primary = adapter.skills_dir();
    if seen.insert(canonical_root(&primary)) {
        roots.push(AgentSkillRoot {
            path: primary,
            read_only: false,
            recursive: adapter.recursive_scan,
        });
    }

    // Already filtered to roots that exist; a missing or unreadable one is
    // simply absent, and nothing here creates it.
    for dir in adapter.additional_existing_scan_dirs() {
        if !seen.insert(canonical_root(&dir)) {
            continue;
        }
        roots.push(AgentSkillRoot {
            path: dir,
            read_only: true,
            // Additional roots keep the flat discovery the global scanner uses.
            recursive: false,
        });
    }

    roots
}

/// Two hits describe the same Skill copied into more than one root: same agent,
/// same name, same enabled state, same bytes. Only then is dropping the
/// lower-precedence one lossless. Deduplicating by name alone would hide a
/// genuine conflict — two same-name Skills whose contents differ are two
/// different Skills, and the user has to see both to resolve them. An unknown
/// content hash never matches, so an unreadable directory is kept, not merged.
fn is_equivalent_result(
    a: &project_scanner::ProjectSkillInfo,
    b: &project_scanner::ProjectSkillInfo,
) -> bool {
    a.agent == b.agent
        && a.name.trim().to_lowercase() == b.name.trim().to_lowercase()
        && a.enabled == b.enabled
        && a.content_hash.is_some()
        && a.content_hash == b.content_hash
}

/// Every Skill the Agent Skills view can show for this agent, across all of the
/// agent's global roots, in precedence order.
fn scan_agent_local_skills(adapter: &tool_adapters::ToolAdapter) -> Vec<ScannedAgentSkill> {
    let mut results: Vec<ScannedAgentSkill> = Vec::new();

    for root in agent_skill_roots(adapter) {
        let skills = project_scanner::read_linked_workspace_skills(
            &root.path,
            None,
            &adapter.key,
            &adapter.display_name,
            root.recursive,
        );
        for skill in skills {
            if results
                .iter()
                .any(|existing| is_equivalent_result(&existing.skill, &skill))
            {
                continue;
            }
            results.push(ScannedAgentSkill {
                skill,
                root: root.path.clone(),
                read_only: root.read_only,
            });
        }
    }

    // Per-root ordering is by name; re-sort so the merged list reads the same
    // way. The sort is stable, so same-name rows keep their precedence order.
    results.sort_by(|a, b| {
        a.skill
            .name
            .to_lowercase()
            .cmp(&b.skill.name.to_lowercase())
    });
    results
}

fn enrich_center_status(
    mut skills: Vec<project_scanner::ProjectSkillInfo>,
    all_managed: &[SkillRecord],
    all_targets: &[SkillTargetRecord],
    tags_map: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<project_scanner::ProjectSkillInfo> {
    for skill in &mut skills {
        let matched = find_verified_center_match(skill, all_managed, all_targets);
        skill.in_center = matched.is_some();
        skill.center_skill_id = matched.map(|record| record.id.clone());
        skill.tags = skill
            .center_skill_id
            .as_ref()
            .and_then(|skill_id| tags_map.get(skill_id).cloned())
            .unwrap_or_default();
        skill.sync_status = classify_sync_status(skill, matched);
    }
    skills
}

/// Resolve the row an action names. The client sends back the absolute path a
/// previous list returned, but that path is never trusted on its own: the agent's
/// roots are re-scanned here and the action proceeds only on an exact hit, using
/// the freshly scanned result rather than anything the client sent. A path that
/// was never listed, one whose Skill or root has since disappeared, and one whose
/// root alias changed all fail closed — there is deliberately no fallback to a
/// same-name Skill in another root, which would silently act on the wrong copy.
fn find_scanned_agent_skill(
    adapter: &tool_adapters::ToolAdapter,
    skill_path: &str,
) -> Result<ScannedAgentSkill, AppError> {
    scan_agent_local_skills(adapter)
        .into_iter()
        .find(|entry| entry.skill.path == skill_path)
        .ok_or_else(|| AppError::not_found("Skill not found in agent local directory"))
}

/// Reject an operation that would write to a discovery-only source. The UI hides
/// these actions on a read-only row, so reaching here means a direct IPC call —
/// it is refused rather than silently redirected at a same-name primary Skill.
fn ensure_writable_source(entry: &ScannedAgentSkill) -> Result<(), AppError> {
    if entry.read_only {
        return Err(AppError::invalid_input(
            "This skill lives in a read-only source directory and cannot be modified.",
        ));
    }
    Ok(())
}

fn ensure_agent_skill_path(path: &Path, skills_root: &Path) -> Result<(), AppError> {
    ensure_dir_within_root(path, skills_root)?;
    Ok(())
}

fn path_matches_skill_path(
    skill_path: &str,
    skill_canonical: Option<&PathBuf>,
    other: &str,
) -> bool {
    if other == skill_path {
        return true;
    }
    let Some(skill_canonical) = skill_canonical else {
        return false;
    };
    let Ok(other_canonical) = std::fs::canonicalize(other) else {
        return false;
    };
    other_canonical == *skill_canonical
}

fn target_matches_skill_path(
    target: &SkillTargetRecord,
    skill_path: &str,
    skill_canonical: Option<&PathBuf>,
) -> bool {
    path_matches_skill_path(skill_path, skill_canonical, &target.target_path)
}

fn find_verified_center_match<'a>(
    skill: &project_scanner::ProjectSkillInfo,
    all_managed: &'a [SkillRecord],
    all_targets: &[SkillTargetRecord],
) -> Option<&'a SkillRecord> {
    let skill_hash = skill.content_hash.as_deref();
    let canonical_skill_path = std::fs::canonicalize(&skill.path).ok();

    all_managed
        .iter()
        .filter_map(|managed| {
            if source_ref_matches_skill_path(&skill.path, canonical_skill_path.as_ref(), managed) {
                return Some((managed, 3));
            }
            if all_targets.iter().any(|target| {
                target.skill_id == managed.id
                    && target_matches_skill_path(target, &skill.path, canonical_skill_path.as_ref())
            }) {
                return Some((managed, 3));
            }
            if skill_hash.is_some() && managed.content_hash.as_deref() == skill_hash {
                return Some((managed, 2));
            }
            None
        })
        .max_by_key(|(_, score)| *score)
        .map(|(managed, _)| managed)
}

#[tauri::command]
pub async fn get_global_local_skills(
    store: State<'_, Arc<SkillStore>>,
    agent: String,
) -> Result<Vec<AgentLocalSkillDto>, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = adapter_for_agent(&store, &agent)?;
        let scanned = scan_agent_local_skills(&adapter);
        let read_only_flags: Vec<bool> = scanned.iter().map(|entry| entry.read_only).collect();
        let skills: Vec<project_scanner::ProjectSkillInfo> =
            scanned.into_iter().map(|entry| entry.skill).collect();
        let all_managed = store.get_all_skills().map_err(AppError::db)?;
        let all_targets = store.get_all_targets().map_err(AppError::db)?;
        let tags_map = store.get_tags_map().unwrap_or_default();
        Ok(
            enrich_center_status(skills, &all_managed, &all_targets, &tags_map)
                .into_iter()
                .zip(read_only_flags)
                .map(|(skill, read_only)| AgentLocalSkillDto { skill, read_only })
                .collect(),
        )
    })
    .await?
}

#[tauri::command]
pub async fn get_global_local_skill_document(
    store: State<'_, Arc<SkillStore>>,
    agent: String,
    skill_path: String,
) -> Result<ProjectSkillDocumentDto, AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = adapter_for_agent(&store, &agent)?;
        let entry = find_scanned_agent_skill(&adapter, &skill_path)?;
        read_agent_skill_document(&entry)
    })
    .await?
}

fn read_agent_skill_document(
    entry: &ScannedAgentSkill,
) -> Result<ProjectSkillDocumentDto, AppError> {
    let skill_dir = PathBuf::from(&entry.skill.path);
    ensure_agent_skill_path(&skill_dir, &entry.root)?;

    let allowed_roots = vec![entry.root.clone()];
    let candidates = ["SKILL.md", "skill.md", "CLAUDE.md", "README.md"];
    for candidate in &candidates {
        let file_path = skill_dir.join(candidate);
        if !file_path.exists() {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(&file_path) {
            if meta.file_type().is_symlink() {
                let resolved = match std::fs::canonicalize(&file_path) {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                let in_allowed_root = allowed_roots.iter().any(|root| {
                    std::fs::canonicalize(root)
                        .map(|canon| resolved.starts_with(&canon))
                        .unwrap_or(false)
                });
                if !in_allowed_root {
                    continue;
                }
            }
        }
        if file_path.is_file() {
            let content = std::fs::read_to_string(&file_path)?;
            return Ok(ProjectSkillDocumentDto {
                skill_name: entry.skill.relative_path.clone(),
                filename: candidate.to_string(),
                content,
            });
        }
    }

    Err(AppError::not_found(
        "No document file found in skill directory",
    ))
}

#[tauri::command]
pub async fn import_global_local_skill_to_center(
    store: State<'_, Arc<SkillStore>>,
    agent: String,
    skill_path: String,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = adapter_for_agent(&store, &agent)?;
        import_agent_local_skill_to_center(&store, &adapter, &skill_path)
    })
    .await?
}

fn import_agent_local_skill_to_center(
    store: &SkillStore,
    adapter: &tool_adapters::ToolAdapter,
    skill_path: &str,
) -> Result<(), AppError> {
    // Importing copies the on-disk skill into the Library. Offline the Library is
    // unverifiable, so any outcome here would be based on a view we cannot trust.
    library_availability::ensure_library_online()?;
    let entry = find_scanned_agent_skill(adapter, skill_path)?;
    let skill = &entry.skill;
    let agent = adapter.key.as_str();

    let source_path = PathBuf::from(&skill.path);
    ensure_agent_skill_path(&source_path, &entry.root)?;

    // A discovery-only source may be copied INTO the Library, and that is where
    // the import stops: registering a sync target would hand the on-disk artifact
    // to sync_engine, which deploys the central copy over the root we promised
    // never to write to. The Library skill still records where it came from.

    let all_managed = store.get_all_skills().unwrap_or_default();
    let all_targets = store.get_all_targets().unwrap_or_default();
    if let Some(existing) = find_verified_center_match(skill, &all_managed, &all_targets) {
        let result = installer::install_from_local_to_destination(
            &source_path,
            Some(&existing.name),
            Path::new(&existing.central_path),
        )
        .map_err(AppError::io)?;
        store
            .update_skill_after_install(
                &existing.id,
                &existing.name,
                result.description.as_deref(),
                existing.source_revision.as_deref(),
                existing.remote_revision.as_deref(),
                Some(&result.content_hash),
                "local_only",
            )
            .map_err(AppError::db)?;

        let already_matched_by_ref = source_ref_matches_skill_path(
            &skill.path,
            std::fs::canonicalize(&skill.path).ok().as_ref(),
            existing,
        );
        if existing.source_type == "local" && already_matched_by_ref {
            store
                .update_skill_source_ref(&existing.id, &skill.path)
                .map_err(AppError::db)?;
        }

        if entry.read_only {
            return Ok(());
        }
        // Register this agent as a managed sync target so the adopted skill is
        // recognized as managed (gives it a delete button). Reusing the regular
        // sync path keeps the target consistent with every other managed skill:
        // sync_engine owns the on-disk artifact, so later unsync/scenario-sync
        // touch only that managed artifact, never the user's source.
        scenario_service::sync_single_skill_to_tool(store, &existing.id, agent)?;
        return Ok(());
    }

    let result =
        installer::install_from_local(&source_path, Some(&skill.name)).map_err(AppError::io)?;
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let skill_record = SkillRecord {
        id,
        name: result.name.clone(),
        description: result.description.clone(),
        source_type: "local".to_string(),
        source_ref: Some(skill.path.clone()),
        source_ref_resolved: None,
        source_subpath: None,
        source_branch: None,
        source_revision: None,
        remote_revision: None,
        central_path: result.central_path.to_string_lossy().to_string(),
        content_hash: Some(result.content_hash.clone()),
        enabled: true,
        created_at: now,
        updated_at: now,
        status: "ok".to_string(),
        update_status: "local_only".to_string(),
        last_checked_at: Some(now),
        last_check_error: None,
    };

    store.insert_skill(&skill_record).map_err(AppError::db)?;
    if entry.read_only {
        return Ok(());
    }
    // Register the managed sync target (see note above). On failure, drop the
    // just-inserted skill row (which cascades to any target) so we never leave
    // an orphaned, button-less skill behind. We deliberately do NOT delete the
    // central directory: `install_from_local` may have de-duplicated onto a
    // directory shared with another skill, and removing it could corrupt that
    // skill — an orphaned dir is harmless by comparison.
    if let Err(err) = scenario_service::sync_single_skill_to_tool(store, &skill_record.id, agent) {
        let _ = store.delete_skill(&skill_record.id);
        return Err(err);
    }
    Ok(())
}

/// Repair "stranded" center skills left behind by uploads that predate the
/// sync-target registration fix. Such a skill has a center record whose
/// `source_ref` still points at a skill living in an agent's skills directory,
/// but no `skill_targets` row for that agent — so the global workspace treats
/// it as in-sync-but-unmanaged and renders no actions (the missing delete
/// button). Runs once at startup; idempotent (after repair the target exists,
/// so later runs find nothing and exit on the cheap pre-check).
///
/// We match strictly by `source_ref` — the strong "this skill was uploaded
/// FROM here" signal — never by content hash, which could silently adopt a
/// look-alike skill the user never uploaded. We also only repair skills whose
/// on-disk content still equals the center copy (hash match): completing the
/// registration runs `sync_single_skill_to_tool`, which rewrites the agent
/// artifact from the central copy, so acting on a diverged skill could clobber
/// newer local edits. Diverged stranded skills are left for an explicit user
/// action. Best-effort: per-skill failures are logged and skipped.
/// Settings key holding the signature of the stranded-candidate set we last
/// attempted to backfill. See [`backfill_stranded_agent_targets`] for why.
const BACKFILL_SIG_KEY: &str = "backfill_stranded_candidates_sig";

/// Change detector for one candidate's on-disk state: the same content hash
/// the repair itself compares (which ignores junk like `.DS_Store`, so noise
/// can't churn the gate), plus an existence marker since `hash_directory`
/// hashes a missing dir like an empty one. Candidates are few, so hashing
/// just their dirs costs ~ms — nothing like the full scan-and-hash of every
/// agent the gate exists to avoid. The gate then re-arms exactly when a
/// candidate's repair inputs changed.
fn dir_content_fingerprint(path: &str) -> String {
    if !Path::new(path).exists() {
        return "missing".to_string();
    }
    content_hash::hash_directory(Path::new(path)).unwrap_or_else(|_| "unreadable".to_string())
}

/// Signature of the current stranded set: skills carrying a non-empty
/// `source_ref` but no target row. Returns `None` when the set is empty (there
/// is nothing to repair). The signature is order-independent (ids are sorted)
/// so it reflects the *set*, not the DB row order.
///
/// Beyond the candidate identities it folds in everything cheap that changes
/// whether a candidate is repairable, so the gate re-arms when repair might
/// newly succeed — not only when the set itself changes: the available
/// (installed + enabled) adapter keys, each candidate's DB content hash, and
/// content fingerprints of its local and central dirs (a diverged local
/// restored to match center, or center edited to match the local, must
/// re-arm).
fn stranded_candidate_signature(
    all_managed: &[SkillRecord],
    all_targets: &[SkillTargetRecord],
    available_tools: &[String],
) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::collections::HashSet;

    let targeted_skill_ids: HashSet<&str> =
        all_targets.iter().map(|t| t.skill_id.as_str()).collect();
    let mut candidates: Vec<(&str, &str, &str, &str)> = all_managed
        .iter()
        .filter_map(|managed| {
            let source_ref = managed.source_ref.as_deref().filter(|s| !s.is_empty())?;
            if targeted_skill_ids.contains(managed.id.as_str()) {
                return None;
            }
            Some((
                managed.id.as_str(),
                source_ref,
                managed.content_hash.as_deref().unwrap_or(""),
                managed.central_path.as_str(),
            ))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_unstable();

    let mut tools: Vec<&str> = available_tools.iter().map(String::as_str).collect();
    tools.sort_unstable();

    let mut hasher = Sha256::new();
    for tool in tools {
        hasher.update(tool.as_bytes());
        hasher.update([0]);
    }
    hasher.update([0xfe]);
    for (id, source_ref, db_hash, central_path) in candidates {
        hasher.update(id.as_bytes());
        hasher.update([0]);
        hasher.update(source_ref.as_bytes());
        hasher.update([0]);
        hasher.update(db_hash.as_bytes());
        for path in [source_ref, central_path] {
            hasher.update([0]);
            hasher.update(dir_content_fingerprint(path).as_bytes());
        }
        hasher.update([0xff]);
    }
    Some(hex::encode(hasher.finalize()))
}

pub fn backfill_stranded_agent_targets(store: &SkillStore) -> usize {
    let all_managed = store.get_all_skills().unwrap_or_default();
    let all_targets = store.get_all_targets().unwrap_or_default();

    let disabled = tool_service::get_disabled_tools(store);
    let adapters = tool_adapters::all_tool_adapters(store);
    let available_tools: Vec<String> = adapters
        .iter()
        .filter(|adapter| adapter.is_installed() && !disabled.contains(&adapter.key))
        .map(|adapter| adapter.key.clone())
        .collect();

    // Cheap in-memory pre-check: a stranded skill carries a `source_ref` but has
    // no target row. When nothing is stranded there is nothing to repair, so we
    // skip the filesystem scan entirely.
    let Some(signature) =
        stranded_candidate_signature(&all_managed, &all_targets, &available_tools)
    else {
        return 0;
    };

    // Second gate (#248): the scan below reads and hashes every agent's local
    // skills - ~8s on real libraries. Some candidates can never be repaired
    // (diverged locals we intentionally skip, or skills with no matching local
    // file), so `has_candidate` alone stayed true forever and re-ran the full
    // scan on EVERY launch. Skip when the stranded set is byte-identical to the
    // one we already attempted; a newly-stranded skill, a newly available
    // adapter, or an on-disk change to a candidate (see the signature docs)
    // re-arms the scan.
    if store
        .get_setting(BACKFILL_SIG_KEY)
        .ok()
        .flatten()
        .as_deref()
        == Some(signature.as_str())
    {
        return 0;
    }

    let mut repaired = 0usize;

    for adapter in &adapters {
        if !adapter.is_installed() || disabled.contains(&adapter.key) {
            continue;
        }
        let targets = store.get_all_targets().unwrap_or_default();

        for skill in read_agent_primary_skills(adapter) {
            let canonical = std::fs::canonicalize(&skill.path).ok();
            let Some(matched) = all_managed.iter().find(|managed| {
                source_ref_matches_skill_path(&skill.path, canonical.as_ref(), managed)
            }) else {
                continue;
            };

            if targets
                .iter()
                .any(|t| t.skill_id == matched.id && t.tool == adapter.key)
            {
                continue;
            }

            // Only safe when the local copy still equals center: the sync below
            // rewrites the agent artifact from central, so a diverged local would
            // lose its newer edits. Reuse the workspace's own classifier (which
            // also recomputes the live center hash when the DB hash is stale) so
            // we repair exactly the skills the UI shows as in-sync, no more.
            if classify_sync_status(&skill, Some(matched)) != "in_sync" {
                log::info!(
                    "backfill: skipping diverged stranded skill '{}' on agent '{}' (needs manual action)",
                    matched.name,
                    adapter.key
                );
                continue;
            }

            // The scan snapshot above can be seconds old on a large library, and
            // we now run concurrently with the user (post-#248 the backfill is
            // off the setup path). Re-hash both sides of THIS skill immediately
            // before writing so an edit made since the scan is never clobbered
            // by the central copy. One dir each — cheap. Explicit existence
            // checks because hash_directory treats a missing dir like an empty
            // one instead of failing.
            let local_path = Path::new(&skill.path);
            let center_path = Path::new(&matched.central_path);
            let fresh_match = local_path.exists()
                && center_path.exists()
                && match (
                    content_hash::hash_directory(local_path),
                    content_hash::hash_directory(center_path),
                ) {
                    (Ok(local), Ok(center)) => local == center,
                    // Fail closed: if either side can't be read, don't write.
                    _ => false,
                };
            if !fresh_match {
                log::info!(
                    "backfill: skipping stranded skill '{}' on agent '{}': content changed since scan",
                    matched.name,
                    adapter.key
                );
                continue;
            }

            match scenario_service::sync_single_skill_to_tool(store, &matched.id, &adapter.key) {
                Ok(()) => {
                    repaired += 1;
                    log::info!(
                        "backfill: registered missing sync target for stranded skill '{}' on agent '{}'",
                        matched.name,
                        adapter.key
                    );
                }
                Err(err) => log::warn!(
                    "backfill: failed to repair stranded skill '{}' on agent '{}': {}",
                    matched.name,
                    adapter.key,
                    err
                ),
            }
        }
    }

    if repaired > 0 {
        log::info!("backfill: repaired {repaired} stranded agent skill target(s)");
    }

    // Record the stranded set that remains AFTER repairing so an unchanged set
    // won't trigger the expensive scan again next launch. Recomputed from fresh
    // targets (repairs above added rows) so it converges in a single launch:
    // repaired skills drop out, and only the genuinely un-repairable ones seed
    // the gate. Best-effort: a failed write just means we re-scan next time.
    let post_targets = store.get_all_targets().unwrap_or_default();
    match stranded_candidate_signature(&all_managed, &post_targets, &available_tools) {
        Some(remaining_sig) => {
            let _ = store.set_setting(BACKFILL_SIG_KEY, &remaining_sig);
        }
        // Everything got repaired: clear the gate so a future stranded skill is
        // never masked by a stale signature.
        None => {
            let _ = store.set_setting(BACKFILL_SIG_KEY, "");
        }
    }

    repaired
}

#[tauri::command]
pub async fn update_global_local_skill_from_center(
    store: State<'_, Arc<SkillStore>>,
    agent: String,
    skill_path: String,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = adapter_for_agent(&store, &agent)?;
        update_agent_local_skill_from_center(&store, &adapter, &skill_path)
    })
    .await?
}

fn update_agent_local_skill_from_center(
    store: &SkillStore,
    adapter: &tool_adapters::ToolAdapter,
    skill_path: &str,
) -> Result<(), AppError> {
    // Updating an agent copy reads the Library as the source of truth. Offline the Library is
    // unverifiable, so any outcome here would be based on a view we cannot trust.
    library_availability::ensure_library_online()?;
    let entry = find_scanned_agent_skill(adapter, skill_path)?;
    ensure_writable_source(&entry)?;
    let skill = &entry.skill;
    let agent = adapter.key.as_str();

    let all_managed = store.get_all_skills().unwrap_or_default();
    let all_targets = store.get_all_targets().unwrap_or_default();
    let managed = find_verified_center_match(skill, &all_managed, &all_targets)
        .ok_or_else(|| AppError::not_found("No matching managed skill in center"))?;

    if classify_sync_status(skill, Some(managed)) == "project_newer" {
        return Err(AppError::invalid_input(
            "Local skill is newer than the Skills Center version",
        ));
    }

    let target_path = PathBuf::from(&skill.path);
    ensure_agent_skill_path(&target_path, &entry.root)?;

    let source = PathBuf::from(&managed.central_path);
    let configured_mode = store.get_setting("sync_mode").map_err(AppError::db)?;
    let mode = sync_engine::sync_mode_for_tool(agent, configured_mode.as_deref());
    sync_engine::sync_skill(&source, &target_path, mode).map_err(AppError::io)?;
    Ok(())
}

#[tauri::command]
pub async fn delete_global_local_skill(
    store: State<'_, Arc<SkillStore>>,
    agent: String,
    skill_path: String,
) -> Result<(), AppError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = adapter_for_agent(&store, &agent)?;
        delete_agent_local_skill(&store, &adapter, &skill_path)
    })
    .await?
}

fn delete_agent_local_skill(
    store: &SkillStore,
    adapter: &tool_adapters::ToolAdapter,
    skill_path: &str,
) -> Result<(), AppError> {
    // Deleting an agent copy is a deployment-target mutation. Offline the Library is
    // unverifiable, so any outcome here would be based on a view we cannot trust.
    library_availability::ensure_library_online()?;
    let entry = find_scanned_agent_skill(adapter, skill_path)?;
    ensure_writable_source(&entry)?;
    let skill = &entry.skill;

    let all_managed = store.get_all_skills().unwrap_or_default();
    let all_targets = store.get_all_targets().unwrap_or_default();
    if let Some(managed) = find_verified_center_match(skill, &all_managed, &all_targets) {
        let still_linked = all_targets.iter().any(|t| {
            t.skill_id == managed.id && target_path_equals_skill(&t.target_path, &skill.path)
        });
        if still_linked {
            return Err(AppError::invalid_input(
                "Skill is managed by Skills Center — remove from the agent first.",
            ));
        }
    }

    let target_path = PathBuf::from(&skill.path);
    ensure_agent_skill_path(&target_path, &entry.root)?;
    sync_engine::remove_target(&target_path).map_err(AppError::io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        backfill_stranded_agent_targets, delete_agent_local_skill, enrich_center_status,
        find_scanned_agent_skill, import_agent_local_skill_to_center, read_agent_skill_document,
        scan_agent_local_skills, update_agent_local_skill_from_center,
    };
    use crate::core::content_hash;
    use crate::core::error::ErrorKind;
    use crate::core::project_scanner::ProjectSkillInfo;
    use crate::core::skill_store::{ScenarioRecord, SkillRecord, SkillStore};
    use crate::core::{central_repo, installer, tool_adapters, tool_service};
    use std::collections::HashMap;
    use std::path::Path;

    /// Adapter whose primary root is an absolute temp dir (via the override) and
    /// whose discovery-only roots are absolute temp dirs too — `additional_scan_dirs`
    /// entries are joined onto `$HOME`, and joining an absolute path replaces it.
    fn test_adapter(primary: &Path, additional: &[&Path]) -> tool_adapters::ToolAdapter {
        tool_adapters::ToolAdapter {
            key: "test_agent".to_string(),
            display_name: "Test Agent".to_string(),
            relative_skills_dir: ".test-agent/skills".to_string(),
            relative_detect_dir: ".test-agent".to_string(),
            additional_scan_dirs: additional
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            project_additional_scan_dirs: vec![],
            override_skills_dir: Some(primary.to_string_lossy().to_string()),
            is_custom: true,
            recursive_scan: false,
            project_relative_skills_dir: None,
            category: tool_adapters::ToolCategory::default(),
        }
    }

    fn write_skill(root: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n---\n{body}\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn scan_lists_legacy_only_skill_as_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&primary).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        let legacy_dir = write_skill(&legacy, "legacy-tool", "legacy");

        let adapter = test_adapter(&primary, &[&legacy]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].skill.name, "legacy-tool");
        assert_eq!(scanned[0].skill.path, legacy_dir.to_string_lossy());
        assert!(scanned[0].read_only);
        assert_eq!(scanned[0].root, legacy);
    }

    #[test]
    fn scan_marks_primary_skills_writable() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&legacy).unwrap();
        let primary_dir = write_skill(&primary, "modern-tool", "modern");

        let adapter = test_adapter(&primary, &[&legacy]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].skill.path, primary_dir.to_string_lossy());
        assert!(!scanned[0].read_only);
    }

    #[test]
    fn scan_skips_missing_additional_root_without_creating_it() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let missing = temp.path().join("does-not-exist");
        write_skill(&primary, "modern-tool", "modern");

        let adapter = test_adapter(&primary, &[&missing]);
        let scanned = scan_agent_local_skills(&adapter);

        // The readable root still lists, and the missing one is never created.
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].skill.name, "modern-tool");
        assert!(!missing.exists());
    }

    #[test]
    fn scan_treats_override_onto_legacy_root_as_writable_primary() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("codex-skills");
        let legacy_dir = write_skill(&legacy, "legacy-tool", "legacy");

        // Override resolves to the legacy directory, which is also configured as
        // a discovery-only root.
        let adapter = test_adapter(&legacy, &[&legacy]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].skill.path, legacy_dir.to_string_lossy());
        assert!(!scanned[0].read_only);
    }

    #[test]
    fn scan_prefers_primary_for_identical_copies() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        let primary_dir = write_skill(&primary, "shared-tool", "same body");
        write_skill(&legacy, "shared-tool", "same body");

        let adapter = test_adapter(&primary, &[&legacy]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].skill.path, primary_dir.to_string_lossy());
        assert!(!scanned[0].read_only);
    }

    #[test]
    fn scan_keeps_conflicting_same_name_copies_distinct() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        let primary_dir = write_skill(&primary, "shared-tool", "modern body");
        let legacy_dir = write_skill(&legacy, "shared-tool", "legacy body");

        let adapter = test_adapter(&primary, &[&legacy]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 2);
        let paths: Vec<&str> = scanned.iter().map(|s| s.skill.path.as_str()).collect();
        assert!(paths.contains(&primary_dir.to_string_lossy().as_ref()));
        assert!(paths.contains(&legacy_dir.to_string_lossy().as_ref()));
        // Each row keeps its own root role.
        let primary_row = scanned
            .iter()
            .find(|s| s.skill.path == primary_dir.to_string_lossy())
            .unwrap();
        let legacy_row = scanned
            .iter()
            .find(|s| s.skill.path == legacy_dir.to_string_lossy())
            .unwrap();
        assert!(!primary_row.read_only);
        assert!(legacy_row.read_only);
    }

    #[test]
    fn scan_leaves_primary_results_unchanged_without_additional_roots() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let a = write_skill(&primary, "alpha-tool", "alpha");
        let b = write_skill(&primary, "beta-tool", "beta");

        // An adapter with no discovery-only roots — every other agent.
        let adapter = test_adapter(&primary, &[]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 2);
        assert!(scanned.iter().all(|s| !s.read_only));
        assert_eq!(scanned[0].skill.path, a.to_string_lossy());
        assert_eq!(scanned[1].skill.path, b.to_string_lossy());
    }

    #[test]
    fn document_reads_the_source_the_path_names_not_a_same_name_sibling() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        write_skill(&primary, "shared-tool", "modern body");
        let legacy_dir = write_skill(&legacy, "shared-tool", "legacy body");

        let adapter = test_adapter(&primary, &[&legacy]);
        let entry = find_scanned_agent_skill(&adapter, &legacy_dir.to_string_lossy()).unwrap();
        let doc = read_agent_skill_document(&entry).unwrap();

        // Both rows share `relative_path`, so only the absolute path can pick one.
        assert_eq!(entry.skill.relative_path, "shared-tool");
        assert!(doc.content.contains("legacy body"));
        assert!(!doc.content.contains("modern body"));
    }

    /// Import, pull, and delete all touch either the Library or an agent's
    /// deployment target, so a direct call must be refused while the Library is
    /// offline — the UI disabling those buttons does not cover IPC.
    #[test]
    fn agent_skill_actions_are_refused_while_offline() {
        use crate::core::library_availability::{
            LibraryAvailability, LibraryReason, LibraryState, set_availability,
        };

        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        let skill_dir = write_skill(&primary, "modern-tool", "modern body");
        let adapter = test_adapter(&primary, &[]);
        let path = skill_dir.to_string_lossy().to_string();
        let body_before = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();

        set_availability(LibraryAvailability {
            state: LibraryState::Offline,
            reason: LibraryReason::MissingPath,
            configured_path: temp.path().join("center"),
            library_id: None,
        });

        for err in [
            import_agent_local_skill_to_center(&store, &adapter, &path).unwrap_err(),
            update_agent_local_skill_from_center(&store, &adapter, &path).unwrap_err(),
            delete_agent_local_skill(&store, &adapter, &path).unwrap_err(),
        ] {
            assert_eq!(err.kind, ErrorKind::LibraryOffline);
            assert_eq!(err.message, "missing_path");
        }

        assert!(store.get_all_skills().unwrap().is_empty(), "no Library rows");
        assert!(store.get_all_targets().unwrap().is_empty(), "no target rows");
        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
            body_before,
            "the agent's own copy must be untouched"
        );

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn actions_reject_a_path_that_was_never_scanned() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        write_skill(&primary, "modern-tool", "modern");
        let untrusted = temp.path().join("untrusted");
        write_skill(&untrusted, "skill", "untrusted");

        let adapter = test_adapter(&primary, &[]);
        let err = import_agent_local_skill_to_center(
            &store,
            &adapter,
            &untrusted.join("skill").to_string_lossy(),
        )
        .unwrap_err();

        assert_eq!(err.kind, ErrorKind::NotFound);
        // No same-name fallback, and nothing was written to the Library.
        assert!(store.get_all_skills().unwrap().is_empty());
        assert!(store.get_all_targets().unwrap().is_empty());

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn actions_reject_a_path_that_disappeared_after_listing() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        // A same-name Skill survives in the other root: the stale path must not
        // silently resolve to it.
        write_skill(&primary, "shared-tool", "modern body");
        let legacy_dir = write_skill(&legacy, "shared-tool", "legacy body");

        let adapter = test_adapter(&primary, &[&legacy]);
        let listed_path = legacy_dir.to_string_lossy().to_string();
        assert!(find_scanned_agent_skill(&adapter, &listed_path).is_ok());

        std::fs::remove_dir_all(&legacy_dir).unwrap();

        assert!(find_scanned_agent_skill(&adapter, &listed_path).is_err());
        let err = import_agent_local_skill_to_center(&store, &adapter, &listed_path).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert!(store.get_all_skills().unwrap().is_empty());

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn dto_serializes_skill_fields_and_read_only_into_one_object() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&primary).unwrap();
        write_skill(&legacy, "legacy-tool", "legacy");

        let adapter = test_adapter(&primary, &[&legacy]);
        let entry = scan_agent_local_skills(&adapter).pop().unwrap();
        let dto = super::AgentLocalSkillDto {
            skill: entry.skill,
            read_only: entry.read_only,
        };

        // The frontend reads `read_only` alongside the existing Skill fields, so
        // the flattened shape is the IPC contract, not an implementation detail.
        let json: serde_json::Value = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["name"], "legacy-tool");
        assert!(json["path"].as_str().unwrap().ends_with("legacy-tool"));
        assert_eq!(json["read_only"], true);
        assert!(json.get("skill").is_none());
    }

    #[test]
    fn read_only_document_leaves_the_source_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&primary).unwrap();
        let legacy_dir = write_skill(&legacy, "legacy-tool", "legacy body");
        let before = content_hash::hash_directory(&legacy_dir).unwrap();

        let adapter = test_adapter(&primary, &[&legacy]);
        let entry = find_scanned_agent_skill(&adapter, &legacy_dir.to_string_lossy()).unwrap();
        assert!(entry.read_only);
        let doc = read_agent_skill_document(&entry).unwrap();

        assert!(doc.content.contains("legacy body"));
        assert_eq!(content_hash::hash_directory(&legacy_dir).unwrap(), before);
    }

    #[test]
    fn read_only_import_creates_center_skill_without_target_or_deployment() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&primary).unwrap();
        let legacy_dir = write_skill(&legacy, "legacy-tool", "legacy body");
        let before = content_hash::hash_directory(&legacy_dir).unwrap();

        let adapter = test_adapter(&primary, &[&legacy]);
        import_agent_local_skill_to_center(&store, &adapter, &legacy_dir.to_string_lossy())
            .unwrap();

        // The central Library gains the skill...
        let skills = store.get_all_skills().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "legacy-tool");
        // ...with no managed target, so nothing deploys back over the source.
        assert!(store.get_all_targets().unwrap().is_empty());
        // The legacy source is byte-identical and still a real directory (a
        // managed target would have replaced it with a symlink).
        assert_eq!(content_hash::hash_directory(&legacy_dir).unwrap(), before);
        assert!(!std::fs::symlink_metadata(&legacy_dir)
            .unwrap()
            .file_type()
            .is_symlink());
        // And the primary root gains no copy of it.
        assert!(!primary.join("legacy-tool").exists());

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn read_only_pull_and_delete_are_rejected() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&primary).unwrap();
        let legacy_dir = write_skill(&legacy, "legacy-tool", "legacy body");

        // A center skill with newer content, matched to the legacy source, so the
        // rejection comes from the read-only guard and not from a missing match.
        let center_source = temp.path().join("center-source");
        write_skill(&center_source, "legacy-tool", "center body");
        let existing = installer::install_from_local(&center_source, Some("legacy-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "center".to_string(),
                name: "legacy-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(legacy_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        let adapter = test_adapter(&primary, &[&legacy]);
        let before = content_hash::hash_directory(&legacy_dir).unwrap();
        let path = legacy_dir.to_string_lossy().to_string();

        let pull = update_agent_local_skill_from_center(&store, &adapter, &path).unwrap_err();
        assert_eq!(pull.kind, ErrorKind::InvalidInput);

        let delete = delete_agent_local_skill(&store, &adapter, &path).unwrap_err();
        assert_eq!(delete.kind, ErrorKind::InvalidInput);

        // Both refusals leave the legacy source exactly as it was.
        assert!(legacy_dir.exists());
        assert_eq!(content_hash::hash_directory(&legacy_dir).unwrap(), before);

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn primary_delete_removes_an_unmanaged_local_skill() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let store = SkillStore::new(&temp.path().join("store.db")).unwrap();
        let primary = temp.path().join("agents-skills");
        let legacy = temp.path().join("codex-skills");
        std::fs::create_dir_all(&legacy).unwrap();
        let primary_dir = write_skill(&primary, "modern-tool", "modern body");

        // The read-only guard must not change what a writable primary row does.
        let adapter = test_adapter(&primary, &[&legacy]);
        delete_agent_local_skill(&store, &adapter, &primary_dir.to_string_lossy()).unwrap();

        assert!(!primary_dir.exists());

        central_repo::set_test_base_dir_override(None);
    }

    #[cfg(unix)]
    #[test]
    fn scan_traverses_a_canonical_root_alias_once() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("agents-skills");
        write_skill(&primary, "shared-tool", "shared");
        let alias = temp.path().join("codex-skills");
        std::os::unix::fs::symlink(&primary, &alias).unwrap();

        let adapter = test_adapter(&primary, &[&alias]);
        let scanned = scan_agent_local_skills(&adapter);

        assert_eq!(scanned.len(), 1);
        assert!(!scanned[0].read_only);
        assert_eq!(scanned[0].root, primary);
    }

    #[test]
    fn importing_agent_local_skill_attaches_target_but_not_scenario() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Local test skill\n---\n",
        )
        .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_scenario(&ScenarioRecord {
                id: "active".to_string(),
                name: "Active".to_string(),
                description: None,
                icon: None,
                sort_order: 0,
                created_at: now,
                updated_at: now,
            })
            .unwrap();
        store.set_active_scenario("active").unwrap();

        let adapter = super::adapter_for_agent(&store, "test_agent").unwrap();
        import_agent_local_skill_to_center(&store, &adapter, &skill_dir.to_string_lossy()).unwrap();

        let skills = store.get_all_skills().unwrap();
        assert_eq!(skills.len(), 1);
        // Importing must NOT silently enroll the skill into the active scenario.
        assert!(store
            .get_scenarios_for_skill(&skills[0].id)
            .unwrap()
            .is_empty());
        // But it MUST register a managed sync target for the importing agent,
        // pointed at the skill's actual on-disk location, so the workspace
        // recognizes it as managed and shows its delete button.
        let targets = store.get_all_targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].skill_id, skills[0].id);
        assert_eq!(targets[0].tool, "test_agent");
        assert_eq!(targets[0].target_path, skill_dir.to_string_lossy());

        // The on-disk artifact must be a sync_engine-owned symlink resolving to
        // the central copy — NOT the user's original real directory. This is
        // the property that makes a later unsync safe: removing the target only
        // drops the managed link, leaving the central skill intact.
        let meta = std::fs::symlink_metadata(&skill_dir).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(
            std::fs::canonicalize(&skill_dir).unwrap(),
            std::fs::canonicalize(&skills[0].central_path).unwrap()
        );

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn enriching_agent_local_skills_copies_center_tags() {
        let skill = ProjectSkillInfo {
            name: "local-tool".to_string(),
            dir_name: "local-tool".to_string(),
            relative_path: "local-tool".to_string(),
            description: Some("Agent copy".to_string()),
            path: "/tmp/agent-skills/local-tool".to_string(),
            files: vec![],
            enabled: true,
            agent: "test_agent".to_string(),
            agent_display_name: "Test Agent".to_string(),
            tags: Vec::new(),
            in_center: false,
            sync_status: "project_only".to_string(),
            center_skill_id: None,
            last_modified_at: None,
            content_hash: Some("same-hash".to_string()),
        };

        let managed = SkillRecord {
            id: "center-id".to_string(),
            name: "local-tool".to_string(),
            description: Some("Center copy".to_string()),
            source_type: "local".to_string(),
            source_ref: None,
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: "/tmp/center/local-tool".to_string(),
            content_hash: Some("same-hash".to_string()),
            enabled: true,
            created_at: 0,
            updated_at: 0,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: Some(0),
            last_check_error: None,
        };

        let mut tags_map = HashMap::new();
        tags_map.insert(
            "center-id".to_string(),
            vec!["create".to_string(), "manage".to_string()],
        );

        let enriched = enrich_center_status(vec![skill], &[managed], &[], &tags_map);
        assert_eq!(enriched[0].center_skill_id.as_deref(), Some("center-id"));
        assert_eq!(
            enriched[0].tags,
            vec!["create".to_string(), "manage".to_string()]
        );
    }

    #[test]
    fn importing_agent_local_skill_does_not_overwrite_name_only_center_match() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let center_source = temp.path().join("center-source");
        std::fs::create_dir_all(&center_source).unwrap();
        std::fs::write(
            center_source.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Center copy\n---\ncenter\n",
        )
        .unwrap();
        let existing = installer::install_from_local(&center_source, Some("local-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "existing-center".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(center_source.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nagent\n",
        )
        .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        let adapter = super::adapter_for_agent(&store, "test_agent").unwrap();
        import_agent_local_skill_to_center(&store, &adapter, &skill_dir.to_string_lossy()).unwrap();

        let skills = store.get_all_skills().unwrap();
        assert_eq!(skills.len(), 2);
        let original_content =
            std::fs::read_to_string(std::path::Path::new(&existing.central_path).join("SKILL.md"))
                .unwrap();
        assert!(original_content.contains("Center copy"));
        assert!(skills.iter().any(|skill| skill.name == "local-tool-2"));

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn importing_verified_center_match_reuses_skill_and_attaches_target() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nlocal\n",
        )
        .unwrap();

        // Pre-existing center skill whose source_ref points at the local skill,
        // so the import resolves to a *verified* match (the existing-match
        // branch) rather than creating a duplicate.
        let existing = installer::install_from_local(&skill_dir, Some("local-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "existing-center".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(skill_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        let adapter = super::adapter_for_agent(&store, "test_agent").unwrap();
        import_agent_local_skill_to_center(&store, &adapter, &skill_dir.to_string_lossy()).unwrap();

        // The existing center skill is reused, not duplicated.
        let skills = store.get_all_skills().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "existing-center");

        // And a managed target is attached for the importing agent at the
        // skill's actual on-disk path.
        let targets = store.get_all_targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].skill_id, "existing-center");
        assert_eq!(targets[0].tool, "test_agent");
        assert_eq!(targets[0].target_path, skill_dir.to_string_lossy());

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn backfill_registers_target_for_stranded_in_sync_skill() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nlocal\n",
        )
        .unwrap();

        // A center skill that was uploaded before targets were registered:
        // source_ref points at the agent dir, content matches, but NO target.
        let existing = installer::install_from_local(&skill_dir, Some("local-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "stranded".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(skill_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        // Stranded precondition: no targets at all.
        assert!(store.get_all_targets().unwrap().is_empty());

        let repaired = backfill_stranded_agent_targets(&store);
        assert_eq!(repaired, 1);

        let targets = store.get_all_targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].skill_id, "stranded");
        assert_eq!(targets[0].tool, "test_agent");
        assert_eq!(targets[0].target_path, skill_dir.to_string_lossy());

        // Idempotent: a second run sees the target and repairs nothing.
        assert_eq!(backfill_stranded_agent_targets(&store), 0);
        assert_eq!(store.get_all_targets().unwrap().len(), 1);

        // After a full repair the gate is cleared (empty), not left pointing at
        // a stale set, so a future stranded skill is never masked.
        assert_eq!(
            store
                .get_setting(super::BACKFILL_SIG_KEY)
                .unwrap()
                .as_deref(),
            Some("")
        );

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn stranded_signature_reflects_set_not_row_order() {
        let skill = |id: &str, source_ref: Option<&str>| SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "local".to_string(),
            source_ref: source_ref.map(str::to_string),
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: String::new(),
            content_hash: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        };

        // No source_ref, or already targeted → not stranded → no signature.
        assert_eq!(
            super::stranded_candidate_signature(&[skill("a", None)], &[], &[]),
            None
        );

        let a = skill("a", Some("/x"));
        let b = skill("b", Some("/y"));
        let sig_ab = super::stranded_candidate_signature(&[a.clone(), b.clone()], &[], &[]);
        let sig_ba = super::stranded_candidate_signature(&[b, a], &[], &[]);
        assert!(sig_ab.is_some());
        // Order-independent: same set → same signature.
        assert_eq!(sig_ab, sig_ba);
        // Different set → different signature.
        assert_ne!(
            sig_ab,
            super::stranded_candidate_signature(&[skill("a", Some("/x"))], &[], &[])
        );
        // Same skill id but a changed source path must re-arm the backfill scan;
        // otherwise a repaired/imported path change could remain hidden behind
        // an old startup gate.
        assert_ne!(
            super::stranded_candidate_signature(&[skill("a", Some("/x"))], &[], &[]),
            super::stranded_candidate_signature(&[skill("a", Some("/new-x"))], &[], &[])
        );
        // A tool becoming available (installed/enabled) must re-arm the scan:
        // the unchanged candidate might be repairable on the new tool.
        assert_ne!(
            super::stranded_candidate_signature(&[skill("a", Some("/x"))], &[], &[]),
            super::stranded_candidate_signature(
                &[skill("a", Some("/x"))],
                &[],
                &["claude".to_string()]
            )
        );
        // ...but the adapter ORDER must not matter.
        assert_eq!(
            super::stranded_candidate_signature(
                &[skill("a", Some("/x"))],
                &[],
                &["claude".to_string(), "codex".to_string()]
            ),
            super::stranded_candidate_signature(
                &[skill("a", Some("/x"))],
                &[],
                &["codex".to_string(), "claude".to_string()]
            )
        );
    }

    #[test]
    fn backfill_skips_scan_when_candidate_signature_unchanged() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nlocal\n",
        )
        .unwrap();

        let existing = installer::install_from_local(&skill_dir, Some("local-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "stranded".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(skill_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        // Pre-seed the gate with the CURRENT stranded set's signature, as if a
        // previous launch already attempted (and could not finish repairing) it.
        // Computed exactly the way backfill computes it (same adapter set).
        let disabled = tool_service::get_disabled_tools(&store);
        let available: Vec<String> = tool_adapters::all_tool_adapters(&store)
            .iter()
            .filter(|a| a.is_installed() && !disabled.contains(&a.key))
            .map(|a| a.key.clone())
            .collect();
        let sig = super::stranded_candidate_signature(
            &store.get_all_skills().unwrap(),
            &store.get_all_targets().unwrap(),
            &available,
        )
        .unwrap();
        store.set_setting(super::BACKFILL_SIG_KEY, &sig).unwrap();

        // The skill IS repairable, but the matching gate short-circuits the
        // expensive filesystem scan (#248): nothing is scanned or repaired.
        assert_eq!(backfill_stranded_agent_targets(&store), 0);
        assert!(store.get_all_targets().unwrap().is_empty());

        // Divergence changes the candidate's content fingerprint → the gate
        // re-arms → the scan runs but correctly refuses to repair a diverged
        // local (that could clobber newer local edits); the attempt re-seeds
        // the gate with the diverged state.
        let skill_md = skill_dir.join("SKILL.md");
        let original = std::fs::read(&skill_md).unwrap();
        std::fs::write(
            &skill_md,
            "---\nname: local-tool\ndescription: Agent copy\n---\nedited locally\n",
        )
        .unwrap();
        assert_eq!(backfill_stranded_agent_targets(&store), 0);
        assert!(store.get_all_targets().unwrap().is_empty());

        // Restoring the local copy to match center must re-arm once more and
        // now complete the repair — "candidate set unchanged" is not
        // "repairability unchanged".
        std::fs::write(&skill_md, original).unwrap();
        assert_eq!(backfill_stranded_agent_targets(&store), 1);
        assert_eq!(store.get_all_targets().unwrap().len(), 1);

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn backfill_rearms_when_adapter_becomes_available() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nlocal\n",
        )
        .unwrap();

        let existing = installer::install_from_local(&skill_dir, Some("local-tool")).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        store
            .insert_skill(&SkillRecord {
                id: "stranded".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(skill_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(existing.content_hash.clone()),
                enabled: true,
                created_at: now,
                updated_at: now,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(now),
                last_check_error: None,
            })
            .unwrap();

        // Seed the gate exactly as the previous launch's backfill would have,
        // BEFORE the matching agent is configured (its key absent from the
        // available set).
        let disabled = tool_service::get_disabled_tools(&store);
        let available: Vec<String> = tool_adapters::all_tool_adapters(&store)
            .iter()
            .filter(|a| a.is_installed() && !disabled.contains(&a.key))
            .map(|a| a.key.clone())
            .collect();
        let sig = super::stranded_candidate_signature(
            &store.get_all_skills().unwrap(),
            &store.get_all_targets().unwrap(),
            &available,
        )
        .unwrap();
        store.set_setting(super::BACKFILL_SIG_KEY, &sig).unwrap();

        // Now the agent appears (installed + enabled): the availability change
        // alone must invalidate the gate and let the repair run.
        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        assert_eq!(backfill_stranded_agent_targets(&store), 1);
        assert_eq!(store.get_all_targets().unwrap().len(), 1);

        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn pulling_from_center_rejects_newer_local_skill() {
        let _guard = central_repo::test_base_dir_lock();
        let temp = tempfile::tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(temp.path().join("center")));

        let db_path = temp.path().join("store.db");
        let store = SkillStore::new(&db_path).unwrap();

        let center_source = temp.path().join("center-source");
        std::fs::create_dir_all(&center_source).unwrap();
        std::fs::write(
            center_source.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Center copy\n---\ncenter\n",
        )
        .unwrap();
        let existing = installer::install_from_local(&center_source, Some("local-tool")).unwrap();

        let skills_root = temp.path().join("agent-skills");
        let skill_dir = skills_root.join("local-tool");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: local-tool\ndescription: Agent copy\n---\nagent newer\n",
        )
        .unwrap();

        store
            .set_setting(
                "custom_tools",
                &serde_json::json!([
                    {
                        "key": "test_agent",
                        "display_name": "Test Agent",
                        "skills_dir": skills_root.to_string_lossy(),
                        "project_relative_skills_dir": ".test-agent/skills"
                    }
                ])
                .to_string(),
            )
            .unwrap();

        store
            .insert_skill(&SkillRecord {
                id: "existing-center".to_string(),
                name: "local-tool".to_string(),
                description: existing.description.clone(),
                source_type: "local".to_string(),
                source_ref: Some(skill_dir.to_string_lossy().to_string()),
                source_ref_resolved: None,
                source_subpath: None,
                source_branch: None,
                source_revision: None,
                remote_revision: None,
                central_path: existing.central_path.to_string_lossy().to_string(),
                content_hash: Some(content_hash::hash_directory(&existing.central_path).unwrap()),
                enabled: true,
                created_at: 0,
                updated_at: 0,
                status: "ok".to_string(),
                update_status: "local_only".to_string(),
                last_checked_at: Some(0),
                last_check_error: None,
            })
            .unwrap();

        let adapter = super::adapter_for_agent(&store, "test_agent").unwrap();
        let result =
            update_agent_local_skill_from_center(&store, &adapter, &skill_dir.to_string_lossy());
        assert!(result.is_err());
        let local_content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(local_content.contains("agent newer"));

        central_repo::set_test_base_dir_override(None);
    }
}
