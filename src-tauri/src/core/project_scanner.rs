use serde::Serialize;
use std::path::{Path, PathBuf};

use super::{content_hash, skill_metadata};

/// Lightweight config describing where an agent keeps project-level skills.
#[derive(Debug, Clone)]
pub struct AgentSkillConfig {
    pub key: String,
    pub display_name: String,
    /// Relative path from project root to the skills directory (e.g. ".claude/skills").
    /// This is the only write target: deployment, enable/disable and delete all
    /// resolve against it.
    pub relative_skills_dir: String,
    /// Extra relative paths that are read but never written — legacy locations
    /// an agent used to deploy to, kept visible so existing skills don't vanish
    /// when the default moves.
    pub additional_relative_skills_dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSkillInfo {
    pub name: String,
    pub dir_name: String,
    #[serde(default)]
    pub relative_path: String,
    pub description: Option<String>,
    pub path: String,
    pub files: Vec<String>,
    pub enabled: bool,
    /// Agent key that owns this skill (e.g. "claude_code", "cursor").
    #[serde(default)]
    pub agent: String,
    /// Human-readable agent name (e.g. "Claude Code", "Cursor").
    #[serde(default)]
    pub agent_display_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub in_center: bool,
    #[serde(default)]
    pub sync_status: String,
    #[serde(default)]
    pub center_skill_id: Option<String>,
    #[serde(skip_serializing)]
    pub last_modified_at: Option<i64>,
    #[serde(skip_serializing)]
    pub content_hash: Option<String>,
}

/// Read skills from all configured agents' project-level skill directories.
pub fn read_project_skills(
    project_path: &Path,
    agent_configs: &[AgentSkillConfig],
) -> Vec<ProjectSkillInfo> {
    let mut skills = Vec::new();

    for config in agent_configs {
        // Primary first, then the read-only fallbacks: that order is the
        // precedence used when the same skill turns up in more than one root.
        // Roots that resolve to the same directory (a symlinked legacy path, an
        // override pointing back at it) are visited once.
        let mut visited_roots = std::collections::HashSet::new();
        let roots = std::iter::once(&config.relative_skills_dir)
            .chain(config.additional_relative_skills_dirs.iter());

        for relative in roots {
            let skills_dir = project_path.join(relative);
            let disabled_dir = project_path.join(format!("{relative}-disabled"));

            if visited_roots.insert(canonical_root(&skills_dir)) {
                read_skills_from_dir(
                    &skills_dir,
                    true,
                    &config.key,
                    &config.display_name,
                    &mut skills,
                    true,
                );
            }
            if visited_roots.insert(canonical_root(&disabled_dir)) {
                read_skills_from_dir(
                    &disabled_dir,
                    false,
                    &config.key,
                    &config.display_name,
                    &mut skills,
                    true,
                );
            }
        }
    }

    dedupe_equivalent_skills(&mut skills);
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills
}

/// Drop a result when a byte-identical copy was already found in a
/// higher-precedence root of the same agent, keeping the higher-precedence one.
///
/// Identity is agent + normalized name + enabled state + content hash. The hash
/// is what makes this safe: two roots holding the *same* skill are one skill to
/// the user, but two roots holding different content under one name is a real
/// conflict, and silently keeping only the winner would hide it. The enabled
/// state is part of the identity for the same reason — a skill that is enabled
/// in one root and disabled in another is not one result.
fn dedupe_equivalent_skills(skills: &mut Vec<ProjectSkillInfo>) {
    let mut seen = std::collections::HashSet::new();
    skills.retain(|skill| {
        seen.insert((
            skill.agent.clone(),
            skill.name.to_lowercase(),
            skill.enabled,
            skill.content_hash.clone(),
        ))
    });
}

/// Identity of a scan root for de-duplication. Falls back to the literal path
/// when the directory cannot be resolved (missing or unreadable), so
/// unresolvable roots stay distinct instead of collapsing onto each other.
fn canonical_root(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn read_linked_workspace_skills(
    skills_root: &Path,
    disabled_root: Option<&Path>,
    agent_key: &str,
    agent_display_name: &str,
    recursive: bool,
) -> Vec<ProjectSkillInfo> {
    let mut skills = Vec::new();
    read_skills_from_dir(
        skills_root,
        true,
        agent_key,
        agent_display_name,
        &mut skills,
        recursive,
    );
    if let Some(disabled_root) = disabled_root {
        read_skills_from_dir(
            disabled_root,
            false,
            agent_key,
            agent_display_name,
            &mut skills,
            recursive,
        );
    }
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills
}

fn should_skip_dir(root: &Path, dir: &Path) -> bool {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with('.') {
        return true;
    }

    // Ignore embedded plugin/cache bundle layouts such as:
    // <bundle>/<version>/skills/<skill>/SKILL.md
    // The workspace root itself may be named "skills", so only skip nested
    // container directories that introduce another "skills" subtree.
    dir != root && dir.join("skills").is_dir()
}

fn read_skills_from_dir(
    dir: &Path,
    enabled: bool,
    agent: &str,
    agent_display_name: &str,
    skills: &mut Vec<ProjectSkillInfo>,
    recursive: bool,
) {
    if !dir.is_dir() {
        return;
    }
    let mut visited = std::collections::HashSet::new();
    if let Ok(canon) = std::fs::canonicalize(dir) {
        visited.insert(canon);
    }
    read_skills_from_dir_recursive(
        dir,
        dir,
        enabled,
        agent,
        agent_display_name,
        skills,
        &mut visited,
        recursive,
    );
}

fn read_skills_from_dir_recursive(
    root: &Path,
    current: &Path,
    enabled: bool,
    agent: &str,
    agent_display_name: &str,
    skills: &mut Vec<ProjectSkillInfo>,
    visited: &mut std::collections::HashSet<PathBuf>,
    recursive: bool,
) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if skill_metadata::is_valid_skill_dir(&path) {
            let dir_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let relative_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            let meta = skill_metadata::parse_skill_md(&path);
            let name = meta
                .name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| dir_name.clone());

            let files = list_files(&path);

            // One recursive walk feeds both the content hash and the
            // last-modified time (#248). Previously this ran three separate
            // walks per skill: `hash_directory`, plus `latest_modified_millis`
            // which called `canonicalize()` on every node, so the workspace
            // scan cost scaled at ~3× the necessary syscalls per skill.
            let content_entries = content_hash::list_content_files(&path);
            let content_hash = Some(content_hash::hash_entries(&content_entries));
            let last_modified_at = content_hash::latest_modified_ms(&content_entries);

            skills.push(ProjectSkillInfo {
                name,
                dir_name: dir_name.clone(),
                relative_path,
                description: meta.description,
                path: path.to_string_lossy().to_string(),
                files,
                enabled,
                agent: agent.to_string(),
                agent_display_name: agent_display_name.to_string(),
                tags: Vec::new(),
                in_center: false,
                sync_status: "project_only".to_string(),
                center_skill_id: None,
                last_modified_at,
                content_hash,
            });
            continue;
        }

        // Only check visited set before recursing into namespace dirs
        // to prevent symlink cycles. Skill dirs (above) are leaf nodes and
        // are allowed to alias the same canonical target.

        if !recursive || should_skip_dir(root, &path) {
            continue;
        }

        let canon = match std::fs::canonicalize(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !visited.insert(canon) {
            continue;
        }
        read_skills_from_dir_recursive(
            root,
            &path,
            enabled,
            agent,
            agent_display_name,
            skills,
            visited,
            recursive,
        );
    }
}

/// Scan a root directory for projects containing any agent's skills directory.
pub fn scan_projects_in_dir(
    root: &Path,
    max_depth: usize,
    agent_configs: &[AgentSkillConfig],
) -> Vec<String> {
    let mut results = Vec::new();
    scan_recursive(root, 0, max_depth, agent_configs, &mut results);
    results.sort();
    results
}

fn has_any_agent_skills(dir: &Path, agent_configs: &[AgentSkillConfig]) -> bool {
    agent_configs.iter().any(|config| {
        dir.join(&config.relative_skills_dir).is_dir()
            || config
                .additional_relative_skills_dirs
                .iter()
                .any(|relative| dir.join(relative).is_dir())
    })
}

fn scan_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    agent_configs: &[AgentSkillConfig],
    results: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }

    if has_any_agent_skills(dir, agent_configs) {
        results.push(dir.to_string_lossy().to_string());
        return; // don't recurse into subdirectories of a matched project
    }

    if depth == max_depth {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Skip hidden directories and common non-project dirs
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "__pycache__"
            {
                continue;
            }
            scan_recursive(&path, depth + 1, max_depth, agent_configs, results);
        }
    }
}

fn list_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    files.push(name.to_string_lossy().to_string());
                }
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::{
        read_linked_workspace_skills, read_project_skills, scan_projects_in_dir, AgentSkillConfig,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn write_skill(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\n---\n{body}"),
        )
        .unwrap();
    }

    /// Codex-shaped config: modern primary plus the discovery-only legacy root.
    fn codex_config(primary: &str) -> AgentSkillConfig {
        AgentSkillConfig {
            key: "codex".to_string(),
            display_name: "Codex".to_string(),
            relative_skills_dir: primary.to_string(),
            additional_relative_skills_dirs: vec![".codex/skills".to_string()],
        }
    }

    fn names(skills: &[super::ProjectSkillInfo]) -> Vec<&str> {
        skills.iter().map(|skill| skill.name.as_str()).collect()
    }

    #[test]
    fn reads_both_modern_and_legacy_project_roots() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/modern-tool"),
            "modern-tool",
            "modern",
        );
        write_skill(
            &tmp.path().join(".codex/skills/legacy-tool"),
            "legacy-tool",
            "legacy",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(names(&skills), vec!["legacy-tool", "modern-tool"]);
        // Each result carries the root it was actually found in.
        let legacy = skills.iter().find(|s| s.name == "legacy-tool").unwrap();
        assert_eq!(
            legacy.path,
            tmp.path()
                .join(".codex/skills/legacy-tool")
                .to_string_lossy()
        );
        assert_eq!(legacy.agent, "codex");
    }

    #[test]
    fn reads_disabled_skills_from_the_legacy_root() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".codex/skills-disabled/parked-tool"),
            "parked-tool",
            "parked",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "parked-tool");
        assert!(!skills[0].enabled);
    }

    #[test]
    fn project_override_replaces_the_primary_but_keeps_legacy_discovery() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".custom/codex-skills/custom-tool"),
            "custom-tool",
            "custom",
        );
        write_skill(
            &tmp.path().join(".codex/skills/legacy-tool"),
            "legacy-tool",
            "legacy",
        );
        // The old default is not the write target anymore, so nothing lives there.
        write_skill(
            &tmp.path().join(".agents/skills/unreachable-tool"),
            "unreachable-tool",
            "unreachable",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".custom/codex-skills")]);

        assert_eq!(names(&skills), vec!["custom-tool", "legacy-tool"]);
    }

    #[cfg(unix)]
    #[test]
    fn aliased_project_roots_are_read_once() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/shared-tool"),
            "shared-tool",
            "shared",
        );
        fs::create_dir_all(tmp.path().join(".codex")).unwrap();
        symlink(
            tmp.path().join(".agents/skills"),
            tmp.path().join(".codex/skills"),
        )
        .unwrap();

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(names(&skills), vec!["shared-tool"]);
    }

    #[test]
    fn identical_copies_in_both_roots_produce_one_result_from_the_primary() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/shared-tool"),
            "shared-tool",
            "same body",
        );
        write_skill(
            &tmp.path().join(".codex/skills/shared-tool"),
            "shared-tool",
            "same body",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(skills.len(), 1);
        // Precedence: the deployment primary wins, so that is the path shown.
        assert_eq!(
            skills[0].path,
            tmp.path()
                .join(".agents/skills/shared-tool")
                .to_string_lossy()
        );
        // Nothing was removed from disk — the legacy copy is still there.
        assert!(tmp
            .path()
            .join(".codex/skills/shared-tool/SKILL.md")
            .is_file());
    }

    #[test]
    fn conflicting_copies_in_both_roots_stay_visible() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/shared-tool"),
            "shared-tool",
            "modern body",
        );
        write_skill(
            &tmp.path().join(".codex/skills/shared-tool"),
            "shared-tool",
            "legacy body",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(skills.len(), 2);
        let mut paths: Vec<&str> = skills.iter().map(|skill| skill.path.as_str()).collect();
        paths.sort();
        let mut expected = vec![
            tmp.path()
                .join(".agents/skills/shared-tool")
                .to_string_lossy()
                .to_string(),
            tmp.path()
                .join(".codex/skills/shared-tool")
                .to_string_lossy()
                .to_string(),
        ];
        expected.sort();
        assert_eq!(paths, expected);
    }

    #[test]
    fn an_enabled_and_a_disabled_copy_are_not_merged() {
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/shared-tool"),
            "shared-tool",
            "same body",
        );
        write_skill(
            &tmp.path().join(".codex/skills-disabled/shared-tool"),
            "shared-tool",
            "same body",
        );

        let skills = read_project_skills(tmp.path(), &[codex_config(".agents/skills")]);

        assert_eq!(skills.len(), 2);
        assert_eq!(skills.iter().filter(|skill| skill.enabled).count(), 1);
        assert_eq!(skills.iter().filter(|skill| !skill.enabled).count(), 1);
    }

    #[test]
    fn same_skill_under_two_agents_is_not_merged() {
        // De-duplication is per agent: the same skill deployed for two agents
        // must keep showing up once per agent.
        let tmp = tempdir().unwrap();
        write_skill(
            &tmp.path().join(".agents/skills/shared-tool"),
            "shared-tool",
            "same body",
        );
        write_skill(
            &tmp.path().join(".claude/skills/shared-tool"),
            "shared-tool",
            "same body",
        );

        let configs = vec![
            codex_config(".agents/skills"),
            AgentSkillConfig {
                key: "claude_code".to_string(),
                display_name: "Claude Code".to_string(),
                relative_skills_dir: ".claude/skills".to_string(),
                additional_relative_skills_dirs: Vec::new(),
            },
        ];

        let skills = read_project_skills(tmp.path(), &configs);

        assert_eq!(skills.len(), 2);
        let mut agents: Vec<&str> = skills.iter().map(|skill| skill.agent.as_str()).collect();
        agents.sort();
        assert_eq!(agents, vec!["claude_code", "codex"]);
    }

    #[test]
    fn project_scan_detects_a_workspace_that_only_has_the_legacy_root() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("demo");
        write_skill(
            &project.join(".codex/skills/legacy-tool"),
            "legacy-tool",
            "legacy",
        );

        let found = scan_projects_in_dir(tmp.path(), 2, &[codex_config(".agents/skills")]);

        assert_eq!(found, vec![project.to_string_lossy().to_string()]);
    }

    #[test]
    fn reads_nested_project_skills_recursively() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join(".hermes").join("skills");
        let nested_skill = root.join("research").join("web-search");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(
            nested_skill.join("SKILL.md"),
            "---\nname: Web Search\ndescription: Nested skill\n---\n",
        )
        .unwrap();

        let configs = vec![AgentSkillConfig {
            key: "hermes".to_string(),
            display_name: "Hermes".to_string(),
            relative_skills_dir: ".hermes/skills".to_string(),
            additional_relative_skills_dirs: Vec::new(),
        }];

        let skills = read_project_skills(tmp.path(), &configs);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].dir_name, "web-search");
        assert_eq!(skills[0].relative_path, "research/web-search");
        assert_eq!(skills[0].name, "Web Search");
    }

    #[test]
    fn prefers_skill_dir_over_namespace_parent_dir() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join(".hermes").join("skills");
        let namespace = root.join("research");
        let nested_skill = namespace.join("web-search");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(namespace.join("notes.txt"), "namespace").unwrap();
        fs::write(nested_skill.join("SKILL.md"), "# Nested").unwrap();

        let configs = vec![AgentSkillConfig {
            key: "hermes".to_string(),
            display_name: "Hermes".to_string(),
            relative_skills_dir: ".hermes/skills".to_string(),
            additional_relative_skills_dirs: Vec::new(),
        }];

        let skills = read_project_skills(tmp.path(), &configs);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].relative_path, "research/web-search");
    }

    #[test]
    fn linked_workspace_skips_hidden_dirs_and_embedded_bundle_skills() {
        let tmp = tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        let disabled_root = tmp.path().join("skills-disabled");

        let top_level_skill = skills_root.join("understand");
        fs::create_dir_all(&top_level_skill).unwrap();
        fs::write(
            top_level_skill.join("SKILL.md"),
            "---\nname: understand\n---\n",
        )
        .unwrap();

        let hidden_skill = skills_root
            .join(".claude")
            .join("skills")
            .join("hidden-skill");
        fs::create_dir_all(&hidden_skill).unwrap();
        fs::write(
            hidden_skill.join("SKILL.md"),
            "---\nname: hidden-skill\n---\n",
        )
        .unwrap();

        let embedded_enabled = skills_root
            .join("understand-anything")
            .join("understand-anything")
            .join("311f2ad1aca5")
            .join("skills")
            .join("understand");
        fs::create_dir_all(&embedded_enabled).unwrap();
        fs::write(
            embedded_enabled.join("SKILL.md"),
            "---\nname: understand\n---\n",
        )
        .unwrap();

        let disabled_skill = disabled_root.join("understand-diff");
        fs::create_dir_all(&disabled_skill).unwrap();
        fs::write(
            disabled_skill.join("SKILL.md"),
            "---\nname: understand-diff\n---\n",
        )
        .unwrap();

        let embedded_disabled = disabled_root
            .join("claude-plugins-official")
            .join("superpowers")
            .join("5.0.7")
            .join("skills")
            .join("brainstorming");
        fs::create_dir_all(&embedded_disabled).unwrap();
        fs::write(
            embedded_disabled.join("SKILL.md"),
            "---\nname: brainstorming\n---\n",
        )
        .unwrap();

        let skills = read_linked_workspace_skills(
            &skills_root,
            Some(&disabled_root),
            "linked",
            "Linked",
            true,
        );

        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(names, vec!["understand", "understand-diff"]);
        assert_eq!(
            skills
                .iter()
                .filter(|skill| skill.name == "understand")
                .count(),
            1
        );
        assert!(skills
            .iter()
            .any(|skill| skill.name == "understand" && skill.enabled));
        assert!(skills
            .iter()
            .any(|skill| skill.name == "understand-diff" && !skill.enabled));
    }

    #[test]
    fn linked_workspace_flat_scan_ignores_nested_skills() {
        let tmp = tempdir().unwrap();
        let skills_root = tmp.path().join("skills");

        let top_level_skill = skills_root.join("codex-tool");
        fs::create_dir_all(&top_level_skill).unwrap();
        fs::write(
            top_level_skill.join("SKILL.md"),
            "---\nname: codex-tool\n---\n",
        )
        .unwrap();

        let nested_skill = skills_root.join("vendor").join("nested-tool");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(
            nested_skill.join("SKILL.md"),
            "---\nname: nested-tool\n---\n",
        )
        .unwrap();

        let skills = read_linked_workspace_skills(&skills_root, None, "codex", "Codex", false);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "codex-tool");
    }
}
