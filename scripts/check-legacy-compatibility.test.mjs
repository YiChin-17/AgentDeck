import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// Product trees only. package-lock.json, node_modules and src-tauri/target carry
// upstream package names AgentDeck does not own, so scanning them would report
// findings no change here could fix.
const SCANNED_DIRS = ["src", "src-tauri/src", "scripts"];
const SCANNED_FILES = ["package.json", "src-tauri/tauri.conf.json"];
const SCANNED_EXTENSIONS = new Set([".ts", ".tsx", ".rs", ".mjs", ".json"]);

/**
 * Legacy identifiers that the AgentDeck identity migration must leave alone.
 *
 * Each entry pins one compatibility identifier to the exact declaration site
 * that produces it, so a regression names the file and the contract instead of
 * reporting a repo-wide string count. `mutation` is the rename that would break
 * it — applied to an in-memory overlay only, never to the file on disk — and
 * `parallelRule` is the forbidden-namespace rule that same rename trips.
 */
const LEGACY_CONTRACTS = [
  {
    id: "storage-root-path",
    file: "src-tauri/src/core/central_repo.rs",
    pattern: /\.join\("\.skills-manager"\)/,
    mutation: { from: ".skills-manager", to: ".agentdeck" },
    parallelRule: "parallel-storage-root",
  },
  {
    id: "metadata-dir-path",
    file: "src-tauri/src/core/sync_metadata.rs",
    pattern: /central_repo::skills_dir\(\)\.join\("\.skills-manager"\)/,
    mutation: { from: ".skills-manager", to: ".agentdeck" },
    parallelRule: "parallel-storage-root",
  },
  {
    id: "protocol-file-path",
    file: "src-tauri/src/core/merge/protocol.rs",
    pattern: /PROTOCOL_FILE_REL: &str = "\.skills-manager\/protocol\.json"/,
    mutation: { from: ".skills-manager/", to: ".agentdeck/" },
    parallelRule: "parallel-protocol-tree",
  },
  {
    id: "repo-lock-filename",
    file: "src-tauri/src/core/repo_lock.rs",
    pattern: /LOCK_FILE_NAME: &str = "\.skills-manager\.lock"/,
    mutation: { from: ".skills-manager.lock", to: ".agentdeck.lock" },
  },
  {
    id: "database-filename",
    file: "src-tauri/src/core/central_repo.rs",
    pattern: /self\.state_base\.join\("skills-manager\.db"\)/,
    mutation: { from: "skills-manager.db", to: "agentdeck.db" },
    parallelRule: "parallel-database",
  },
  {
    id: "database-migration-entry",
    file: "src-tauri/src/core/central_repo.rs",
    pattern: /MIGRATED_STATE_ENTRIES: \[&str; 2\] = \["skills-manager\.db", "scenarios"\]/,
    mutation: { from: "skills-manager.db", to: "agentdeck.db" },
    parallelRule: "parallel-database",
  },
  {
    id: "hidden-ref-pre-merge",
    file: "src-tauri/src/core/merge/pending.rs",
    pattern: /REF_PRE_MERGE: &str = "refs\/skills-manager\/pre-merge"/,
    mutation: { from: "refs/skills-manager/", to: "refs/agentdeck/" },
    parallelRule: "parallel-ref-namespace",
  },
  {
    id: "hidden-ref-applying",
    file: "src-tauri/src/core/merge/pending.rs",
    pattern: /REF_APPLYING: &str = "refs\/skills-manager\/applying"/,
    mutation: { from: "refs/skills-manager/", to: "refs/agentdeck/" },
    parallelRule: "parallel-ref-namespace",
  },
  {
    id: "hidden-ref-conflict-prefix",
    file: "src-tauri/src/core/merge/pending.rs",
    pattern: /CONFLICT_REF_PREFIX: &str = "refs\/skills-manager\/conflict\/"/,
    mutation: { from: "refs/skills-manager/", to: "refs/agentdeck/" },
    parallelRule: "parallel-ref-namespace",
  },
  {
    id: "hidden-ref-staging-prefix",
    file: "src-tauri/src/core/merge/pending.rs",
    pattern: /STAGING_REF_PREFIX: &str = "refs\/skills-manager\/conflict-staging\/"/,
    mutation: { from: "refs/skills-manager/", to: "refs/agentdeck/" },
    parallelRule: "parallel-ref-namespace",
  },
  {
    id: "hidden-ref-remote-prune",
    file: "src-tauri/src/core/git_backup.rs",
    pattern: /HIDDEN_PREFIX: &str = "refs\/skills-manager\/"/,
    mutation: { from: "refs/skills-manager/", to: "refs/agentdeck/" },
    parallelRule: "parallel-ref-namespace",
  },
  {
    id: "trailer-protocol",
    file: "src-tauri/src/core/merge/protocol.rs",
    pattern: /TRAILER_PROTOCOL: &str = "Skills-Manager-Protocol"/,
    mutation: { from: "Skills-Manager-", to: "AgentDeck-" },
    parallelRule: "parallel-commit-trailers",
  },
  {
    id: "trailer-conflicts",
    file: "src-tauri/src/core/merge/protocol.rs",
    pattern: /TRAILER_CONFLICTS: &str = "Skills-Manager-Conflicts"/,
    mutation: { from: "Skills-Manager-", to: "AgentDeck-" },
    parallelRule: "parallel-commit-trailers",
  },
  {
    id: "trailer-resolved",
    file: "src-tauri/src/core/merge/protocol.rs",
    pattern: /TRAILER_RESOLVED: &str = "Skills-Manager-Resolved"/,
    mutation: { from: "Skills-Manager-", to: "AgentDeck-" },
    parallelRule: "parallel-commit-trailers",
  },
  {
    id: "keychain-service",
    file: "src-tauri/src/core/git_credentials.rs",
    pattern: /KEYRING_SERVICE: &str = "skills-manager-git-backup"/,
    mutation: { from: "skills-manager-git-backup", to: "agentdeck-git-backup" },
    parallelRule: "parallel-keychain-service",
  },
  {
    id: "localstorage-tool-order",
    file: "src/components/Sidebar.tsx",
    pattern: /"skills-manager:tool-order"/,
    mutation: { from: '"skills-manager:', to: '"agentdeck:' },
    parallelRule: "parallel-localstorage-key",
  },
  {
    id: "localstorage-lobster-tool-order",
    file: "src/components/Sidebar.tsx",
    pattern: /"skills-manager:lobster-tool-order"/,
    mutation: { from: '"skills-manager:', to: '"agentdeck:' },
    parallelRule: "parallel-localstorage-key",
  },
  {
    id: "localstorage-viewed-preset",
    file: "src/context/AppContext.tsx",
    pattern: /VIEWED_PRESET_LS_KEY = "skills-manager\.viewedPresetId"/,
    mutation: { from: '"skills-manager.', to: '"agentdeck.' },
    parallelRule: "parallel-localstorage-key",
  },
  {
    id: "localstorage-legacy-viewed-preset",
    file: "src/context/AppContext.tsx",
    pattern: /LEGACY_VIEWED_PRESET_LS_KEY = "skills-manager\.viewedScenarioId"/,
    mutation: { from: '"skills-manager.', to: '"agentdeck.' },
    parallelRule: "parallel-localstorage-key",
  },
  {
    id: "localstorage-project-add-callout",
    file: "src/views/ProjectDetail.tsx",
    pattern: /PROJECT_ADD_CALLOUT_KEY = "skills-manager\.projectAddCalloutDismissed"/,
    mutation: { from: '"skills-manager.', to: '"agentdeck.' },
    parallelRule: "parallel-localstorage-key",
  },
  {
    // Cargo derives the binary name from this filename, so the file moving or
    // disappearing is what actually renames `skills-manager-cli`.
    id: "cli-binary-source",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /fn main\(\) \{/,
    mutation: { delete: true },
  },
  {
    id: "cli-command-name",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /#\[command\(name = "skills-manager-cli"\)\]/,
    mutation: { from: 'name = "skills-manager-cli"', to: 'name = "agentdeck-cli"' },
  },
  {
    id: "cli-runner-build-bin",
    file: "scripts/run-rust-cli.mjs",
    pattern: /const baseArgs = \['--manifest-path', 'src-tauri\/Cargo\.toml', '--bin', 'skills-manager-cli'\]/,
    mutation: { from: "'skills-manager-cli'", to: "'agentdeck-cli'" },
  },
  {
    id: "cli-runner-install-bin",
    file: "scripts/run-rust-cli.mjs",
    pattern: /'install', '--path', 'src-tauri', '--bin', 'skills-manager-cli', '--locked', '--force'/,
    mutation: { from: "'skills-manager-cli'", to: "'agentdeck-cli'" },
  },
  {
    id: "cli-runner-modes",
    file: "scripts/run-rust-cli.mjs",
    pattern: /mode === 'cli'[\s\S]*mode === 'build'[\s\S]*mode === 'install'/,
    mutation: { from: "mode === 'install'", to: "mode === 'setup'" },
  },
  {
    id: "cli-package-scripts",
    file: "package.json",
    pattern: /"cli": "node scripts\/run-rust-cli\.mjs cli",\s*"cli:build": "node scripts\/run-rust-cli\.mjs build",\s*"cli:install": "node scripts\/run-rust-cli\.mjs install"/,
    mutation: { from: '"cli:build"', to: '"agentdeck:build"' },
  },
  {
    id: "cli-json-flag",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /#\[arg\(long, global = true\)\]\s*\n\s*json: bool,/,
    mutation: { from: "json: bool,", to: "output_json: bool," },
  },
  {
    id: "cli-skills-root-flag",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /#\[arg\(long, global = true\)\]\s*\n\s*skills_root: Option<PathBuf>,/,
    mutation: { from: "skills_root: Option<PathBuf>,", to: "library_root: Option<PathBuf>," },
  },
  {
    id: "cli-json-error-envelope",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /serde_json::json!\(\{"ok": false, "error": format!\("\{err:#\}"\)\}\)/,
    mutation: { from: '"ok": false', to: '"success": false' },
  },
  {
    id: "cli-json-compact-output",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /fn print_json<T: Serialize>[\s\S]*?serde_json::to_string\(value\)\.unwrap\(\)/,
    mutation: { from: "serde_json::to_string(value).unwrap()", to: "String::new()" },
  },
  {
    id: "cli-repo-status-shape",
    file: "src-tauri/src/bin/skills-manager-cli.rs",
    pattern: /struct RepoStatus \{\s*base_dir: String,\s*skills_dir: String,\s*db_path: String,\s*metadata_dir: String,\s*skill_count: usize,\s*preset_count: usize,\s*active_preset_id: Option<String>,\s*\}/,
    mutation: { from: "db_path: String,", to: "database_path: String," },
  },
  {
    id: "skill-pack-cli-contract",
    file: "skills/manage-skills/SKILL.md",
    pattern: /skills-manager-cli --json skills list/,
    mutation: { from: "skills-manager-cli", to: "agentdeck-cli" },
  },
];

/**
 * Namespaces the migration must not create alongside the legacy ones.
 *
 * Deliberately narrow: `.agentdeck-library.json` (the external-library marker)
 * and the `io.github.yichin17.agentdeck` Bundle ID are legitimate AgentDeck
 * identifiers and must keep passing, so no rule may match a bare `agentdeck`.
 */
const PARALLEL_RULES = [
  { id: "parallel-storage-root", pattern: /join\("\.agentdeck"\)/ },
  { id: "parallel-protocol-tree", pattern: /\.agentdeck\// },
  { id: "parallel-database", pattern: /\bagentdeck\.db\b/ },
  { id: "parallel-ref-namespace", pattern: /refs\/agentdeck\// },
  { id: "parallel-commit-trailers", pattern: /"AgentDeck-(?:Protocol|Conflicts|Resolved)"/ },
  { id: "parallel-keychain-service", pattern: /agentdeck-git-backup/ },
  { id: "parallel-localstorage-key", pattern: /["']agentdeck[.:]/ },
];

function walk(relativeDir) {
  const out = [];
  for (const entry of fs.readdirSync(path.join(root, relativeDir), { withFileTypes: true })) {
    const relativePath = `${relativeDir}/${entry.name}`;
    if (entry.isDirectory()) {
      out.push(...walk(relativePath));
    } else if (SCANNED_EXTENSIONS.has(path.extname(entry.name))) {
      out.push(relativePath);
    }
  }
  return out;
}

/**
 * Repository view with optional in-memory file overrides.
 *
 * Mutation fixtures replace file contents here instead of on disk, so proving
 * an assertion fails on a violation never writes to a production file. An
 * override of `null` stands for a deleted file.
 */
function makeRepo(overrides = {}) {
  const read = (relativePath) => {
    if (relativePath in overrides) return overrides[relativePath] ?? "";
    const absolute = path.join(root, relativePath);
    return fs.existsSync(absolute) ? fs.readFileSync(absolute, "utf8") : "";
  };

  const scannedFiles = () => {
    const seen = new Set([...SCANNED_DIRS.flatMap(walk), ...SCANNED_FILES, ...Object.keys(overrides)]);
    return [...seen]
      // Test files quote the forbidden namespaces as fixtures; scanning them
      // would report this file's own mutation strings as product violations.
      .filter((relativePath) => !relativePath.endsWith(".test.mjs"))
      .filter((relativePath) => overrides[relativePath] !== null);
  };

  return { read, scannedFiles };
}

function missingLegacyIdentifiers(repo) {
  return LEGACY_CONTRACTS.flatMap((contract) =>
    contract.pattern.test(repo.read(contract.file))
      ? []
      : [`${contract.id}: ${contract.file} no longer declares ${contract.pattern}`],
  );
}

function parallelNamespaceViolations(repo) {
  return repo.scannedFiles().flatMap((relativePath) => {
    const source = repo.read(relativePath);
    return PARALLEL_RULES.flatMap((rule) => {
      const match = source.match(rule.pattern);
      return match ? [`${rule.id}: ${relativePath}: ${match[0]}`] : [];
    });
  });
}

function mutate(contract) {
  if (contract.mutation.delete) return { [contract.file]: null };
  const source = fs.readFileSync(path.join(root, contract.file), "utf8");
  const mutated = source.split(contract.mutation.from).join(contract.mutation.to);
  assert.notEqual(mutated, source, `mutation for ${contract.id} changed nothing`);
  return { [contract.file]: mutated };
}

test("legacy storage, database and protocol identifiers are unchanged", () => {
  const repo = makeRepo();
  const storageContracts = [
    "storage-root-path",
    "metadata-dir-path",
    "protocol-file-path",
    "repo-lock-filename",
    "database-filename",
    "database-migration-entry",
  ];
  const missing = missingLegacyIdentifiers(repo).filter((entry) =>
    storageContracts.some((id) => entry.startsWith(`${id}:`)),
  );

  assert.deepEqual(missing, []);
});

test("hidden git refs, commit trailers and Keychain service are unchanged", () => {
  const repo = makeRepo();
  const protocolContracts = [
    "hidden-ref-pre-merge",
    "hidden-ref-applying",
    "hidden-ref-conflict-prefix",
    "hidden-ref-staging-prefix",
    "hidden-ref-remote-prune",
    "trailer-protocol",
    "trailer-conflicts",
    "trailer-resolved",
    "keychain-service",
  ];
  const missing = missingLegacyIdentifiers(repo).filter((entry) =>
    protocolContracts.some((id) => entry.startsWith(`${id}:`)),
  );

  assert.deepEqual(missing, []);
});

test("existing localStorage keys are unchanged", () => {
  const repo = makeRepo();
  const missing = missingLegacyIdentifiers(repo).filter((entry) =>
    entry.startsWith("localstorage-"),
  );

  assert.deepEqual(missing, []);
});

test("skills-manager-cli binary, runner and JSON contract are unchanged", () => {
  const repo = makeRepo();
  const missing = missingLegacyIdentifiers(repo).filter(
    (entry) => entry.startsWith("cli-") || entry.startsWith("skill-pack-cli-"),
  );

  assert.deepEqual(missing, []);
  assert.ok(fs.existsSync(path.join(root, "src-tauri/src/bin/skills-manager-cli.rs")));
});

test("every pinned legacy identifier is present", () => {
  assert.deepEqual(missingLegacyIdentifiers(makeRepo()), []);
});

test("no parallel AgentDeck protocol tree, refs, storage keys or Keychain service exists", () => {
  const repo = makeRepo();

  assert.deepEqual(parallelNamespaceViolations(repo), []);
});

test("the parallel-namespace scan actually covers the product trees", () => {
  const scanned = makeRepo().scannedFiles();

  // Without this the parallel-namespace assertion could pass by scanning nothing.
  assert.ok(scanned.length > 50, `only ${scanned.length} files scanned`);
  for (const expected of [
    "src/components/Sidebar.tsx",
    "src-tauri/src/core/merge/protocol.rs",
    "src-tauri/src/bin/skills-manager-cli.rs",
    "scripts/run-rust-cli.mjs",
    "package.json",
  ]) {
    assert.ok(scanned.includes(expected), `${expected} not scanned`);
  }
  assert.ok(!scanned.some((relativePath) => relativePath.endsWith(".test.mjs")));
});

test("legitimate AgentDeck identifiers are not treated as a parallel namespace", () => {
  // These already ship: the external-library marker and the fixed Bundle ID.
  // A rule that flags them would force the migration to rename real AgentDeck
  // assets, so they are pinned as passing input.
  const repo = makeRepo({
    "src-tauri/src/core/library_availability.rs": [
      'pub const MARKER_FILE_NAME: &str = ".agentdeck-library.json";',
      'const BUNDLE_ID: &str = "io.github.yichin17.agentdeck";',
    ].join("\n"),
  });

  assert.deepEqual(parallelNamespaceViolations(repo), []);
});

for (const contract of LEGACY_CONTRACTS) {
  test(`${contract.id} mutation fails and names its file`, () => {
    const violations = missingLegacyIdentifiers(makeRepo(mutate(contract)));

    assert.ok(
      violations.some((entry) => entry.startsWith(`${contract.id}:`) && entry.includes(contract.file)),
      `expected ${contract.id} violation, got ${JSON.stringify(violations)}`,
    );
  });
}

for (const contract of LEGACY_CONTRACTS.filter((entry) => entry.parallelRule)) {
  test(`${contract.id} mutation is reported as ${contract.parallelRule}`, () => {
    const violations = parallelNamespaceViolations(makeRepo(mutate(contract)));

    assert.ok(
      violations.some(
        (entry) => entry.startsWith(`${contract.parallelRule}:`) && entry.includes(contract.file),
      ),
      `expected ${contract.parallelRule} in ${contract.file}, got ${JSON.stringify(violations)}`,
    );
  });
}
