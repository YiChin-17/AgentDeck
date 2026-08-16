import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { RELEASE_FILES, bumpVersion, releaseBlockers } from "./prepare-release.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const scriptPath = path.join(root, "scripts", "prepare-release.mjs");
const PREPARE_WORKFLOW = ".github/workflows/prepare-release.yml";

const VERSION = "1.31.0";
const NEXT_VERSION = "1.31.1";

function localeFixture(version) {
  return JSON.stringify({ app: { name: "AgentDeck" }, settings: { version: `AgentDeck ${version}` } }, null, 2);
}

function defaultFixture() {
  return {
    "package.json": JSON.stringify({ name: "agentdeck", version: VERSION }, null, 2),
    "src-tauri/tauri.conf.json": JSON.stringify(
      { productName: "AgentDeck", version: VERSION, identifier: "io.github.yichin17.agentdeck" },
      null,
      2,
    ),
    "src/i18n/en.json": localeFixture(VERSION),
    "src/i18n/zh-TW.json": localeFixture(VERSION),
    "CHANGELOG.md": `# Changelog\n\n## [${VERSION}] - 2026-08-01\n\n### Release Overview\n\n- Something\n`,
    "CHANGELOG-zh.md": `# 变更日志\n\n## [${VERSION}] - 2026-08-01\n\n### 发布概览\n\n- 某些事\n`,
    "assets/star-history.svg": "<svg></svg>\n",
  };
}

function git(fixtureRoot, args) {
  const result = spawnSync("git", args, { cwd: fixtureRoot, encoding: "utf8" });
  assert.equal(result.status, 0, `git ${args.join(" ")}: ${result.stdout}${result.stderr}`);
  return result.stdout;
}

/// The script refuses to prepare a release outside protected main history or on
/// a reused tag, so the fixture has to be a real repository for those guards to
/// be exercised rather than mocked away.
function withFixture(mutate, callback) {
  const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-prepare-release-"));
  try {
    const files = defaultFixture();
    mutate?.(files);
    for (const [relativePath, contents] of Object.entries(files)) {
      if (contents === null) continue;
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, contents, "utf8");
    }
    git(fixtureRoot, ["init", "-q", "-b", "main"]);
    git(fixtureRoot, ["add", "-A"]);
    git(fixtureRoot, [
      "-c",
      "user.name=fixture",
      "-c",
      "user.email=fixture@example.invalid",
      "commit",
      "-q",
      "-m",
      "fixture",
    ]);
    return callback(fixtureRoot);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

function runScript(fixtureRoot, args) {
  return spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: fixtureRoot,
    encoding: "utf8",
  });
}

function readJson(fixtureRoot, relativePath) {
  return JSON.parse(fs.readFileSync(path.join(fixtureRoot, relativePath), "utf8"));
}

function blockerCodes(overrides) {
  return releaseBlockers({
    nextVersion: NEXT_VERSION,
    branch: "main",
    tags: [`v${VERSION}`],
    packageVersion: VERSION,
    bundleVersion: VERSION,
    productName: "AgentDeck",
    ...overrides,
  }).map((blocker) => blocker.code);
}

test("bumpVersion derives the next version from the release type", () => {
  assert.equal(bumpVersion("1.31.0", "patch"), "1.31.1");
  assert.equal(bumpVersion("1.31.0", "minor"), "1.32.0");
  assert.equal(bumpVersion("1.31.0", "major"), "2.0.0");
  assert.equal(bumpVersion("1.31.0", "2.5.7"), "2.5.7");
  assert.throws(() => bumpVersion("1.31.0", "nightly"), /Invalid release type/);
});

test("a releasable repository on main has no blockers", () => {
  assert.deepEqual(blockerCodes(), []);
});

test("a version that already has a tag is blocked", () => {
  assert.deepEqual(blockerCodes({ tags: [`v${VERSION}`, `v${NEXT_VERSION}`] }), ["tag_reused"]);
});

test("preparing outside the protected main branch is blocked", () => {
  assert.deepEqual(blockerCodes({ branch: "feature/x" }), ["unprotected_branch"]);
});

test("committed versions that already disagree are blocked", () => {
  assert.deepEqual(blockerCodes({ bundleVersion: "1.30.0" }), ["version_mismatch"]);
});

test("a bundle that is not AgentDeck is blocked", () => {
  assert.deepEqual(blockerCodes({ productName: "Skills Manager" }), ["identity_mismatch"]);
});

test("a target version that is not SemVer is blocked", () => {
  assert.deepEqual(blockerCodes({ nextVersion: "1.31" }), ["invalid_version"]);
});

test("the release file list carries the current locale files only", () => {
  assert.ok(RELEASE_FILES.includes("src/i18n/zh-TW.json"));
  assert.ok(RELEASE_FILES.includes("src/i18n/en.json"));
  assert.ok(!RELEASE_FILES.includes("src/i18n/zh.json"));
  assert.ok(RELEASE_FILES.includes("assets/star-history.svg"));
  for (const relativePath of RELEASE_FILES) {
    assert.ok(fs.existsSync(path.join(root, relativePath)), `${relativePath} does not exist`);
  }
});

test("--list-files prints the staged release files one per line", () => {
  const result = withFixture(null, (fixtureRoot) => runScript(fixtureRoot, ["--list-files"]));

  assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
  assert.deepEqual(result.stdout.trim().split("\n"), RELEASE_FILES);
});

test("a dry run reports the bump and writes nothing", () => {
  const result = withFixture(null, (fixtureRoot) => {
    const run = runScript(fixtureRoot, ["patch", "--dry-run"]);
    return {
      run,
      packageVersion: readJson(fixtureRoot, "package.json").version,
      bundleVersion: readJson(fixtureRoot, "src-tauri/tauri.conf.json").version,
    };
  });

  assert.equal(result.run.status, 0, `${result.run.stdout}${result.run.stderr}`);
  assert.match(result.run.stdout, new RegExp(`\\[dry-run\\] ${VERSION} -> ${NEXT_VERSION}`));
  assert.equal(result.packageVersion, VERSION);
  assert.equal(result.bundleVersion, VERSION);
});

test("a prepared release moves every committed version together", () => {
  const prepared = withFixture(null, (fixtureRoot) => {
    const run = runScript(fixtureRoot, ["patch"]);
    return {
      run,
      packageVersion: readJson(fixtureRoot, "package.json").version,
      bundleVersion: readJson(fixtureRoot, "src-tauri/tauri.conf.json").version,
      en: readJson(fixtureRoot, "src/i18n/en.json").settings.version,
      zhTw: readJson(fixtureRoot, "src/i18n/zh-TW.json").settings.version,
      changelog: fs.readFileSync(path.join(fixtureRoot, "CHANGELOG.md"), "utf8"),
      changelogZh: fs.readFileSync(path.join(fixtureRoot, "CHANGELOG-zh.md"), "utf8"),
    };
  });

  assert.equal(prepared.run.status, 0, `${prepared.run.stdout}${prepared.run.stderr}`);
  assert.equal(prepared.packageVersion, NEXT_VERSION);
  assert.equal(prepared.bundleVersion, NEXT_VERSION);
  assert.equal(prepared.en, `AgentDeck ${NEXT_VERSION}`);
  assert.equal(prepared.zhTw, `AgentDeck ${NEXT_VERSION}`);
  assert.match(prepared.changelog, new RegExp(`## \\[${NEXT_VERSION}\\] - `));
  assert.match(prepared.changelogZh, new RegExp(`## \\[${NEXT_VERSION}\\] - `));
});

test("a repository missing a release file fails before writing anything", () => {
  const result = withFixture(
    (files) => {
      files["src/i18n/zh-TW.json"] = null;
    },
    (fixtureRoot) => ({
      run: runScript(fixtureRoot, ["patch"]),
      packageVersion: readJson(fixtureRoot, "package.json").version,
    }),
  );

  assert.notEqual(result.run.status, 0);
  assert.match(`${result.run.stdout}${result.run.stderr}`, /src\/i18n\/zh-TW\.json/);
  assert.equal(result.packageVersion, VERSION);
});

test("preparing a version whose tag already exists writes nothing", () => {
  const result = withFixture(null, (fixtureRoot) => {
    git(fixtureRoot, ["tag", `v${NEXT_VERSION}`]);
    return {
      run: runScript(fixtureRoot, ["patch"]),
      packageVersion: readJson(fixtureRoot, "package.json").version,
    };
  });

  assert.notEqual(result.run.status, 0);
  assert.match(`${result.run.stdout}${result.run.stderr}`, /tag_reused/);
  assert.equal(result.packageVersion, VERSION);
});

test("preparing outside main writes nothing", () => {
  const result = withFixture(null, (fixtureRoot) => {
    git(fixtureRoot, ["checkout", "-q", "-b", "feature/side-quest"]);
    return {
      run: runScript(fixtureRoot, ["patch"]),
      packageVersion: readJson(fixtureRoot, "package.json").version,
    };
  });

  assert.notEqual(result.run.status, 0);
  assert.match(`${result.run.stdout}${result.run.stderr}`, /unprotected_branch/);
  assert.equal(result.packageVersion, VERSION);
});

test("the prepare workflow stages exactly the files the script writes", () => {
  const workflow = fs.readFileSync(path.join(root, PREPARE_WORKFLOW), "utf8");

  assert.match(workflow, /--list-files/);
  assert.doesNotMatch(workflow, /src\/i18n\/zh\.json/);
});

test("the prepare workflow refuses a reused tag and a non-main history", () => {
  const workflow = fs.readFileSync(path.join(root, PREPARE_WORKFLOW), "utf8");

  assert.match(workflow, /refs\/tags\/v/);
  assert.match(workflow, /github\.ref_name == 'main'|refs\/heads\/main/);
});
