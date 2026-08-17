import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  REQUIRED_DESKTOP_ASSETS,
  checkProductIdentity,
} from "./check-product-identity.mjs";

const upstreamIcon = Buffer.from("upstream-icon-fixture");
const upstreamIconSha256 = crypto.createHash("sha256").update(upstreamIcon).digest("hex");
const checkerPath = fileURLToPath(new URL("./check-product-identity.mjs", import.meta.url));
const UPSTREAM_REPO_URL = "https://github.com/xingkongliang/skills-manager";

/// Mirrors the two managed Settings destinations: the repository button opens the
/// constant itself, the report-issue flow appends the bug report template to it.
function settingsSource({
  repositoryBase = "https://github.com/YiChin-17/AgentDeck",
  repositoryAction = "await openUrl(GITHUB_URL);",
  issueAction = "await openUrl(`${GITHUB_URL}/issues/new?template=bug_report.md`);",
} = {}) {
  return [
    "// diagnostics are auto-collected by AgentDeck",
    `  const GITHUB_URL = "${repositoryBase}";`,
    `      ${repositoryAction}`,
    `      ${issueAction}`,
  ].join("\n");
}

function writeFixture(overrides = {}) {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-identity-"));
  const files = {
    "package.json": JSON.stringify({ name: "agentdeck" }),
    "package-lock.json": JSON.stringify({ name: "agentdeck", packages: { "": { name: "agentdeck" } } }),
    "src-tauri/tauri.conf.json": JSON.stringify({
      productName: "AgentDeck",
      identifier: "io.github.yichin17.agentdeck",
      app: { windows: [{ title: "AgentDeck" }] },
    }),
    "src-tauri/Cargo.toml": [
      "[package]",
      'name = "agentdeck"',
      'default-run = "agentdeck"',
      "[lib]",
      'name = "app_lib"',
      "[[bin]]",
      'name = "skills-manager-cli"',
    ].join("\n"),
    "index.html": "<title>AgentDeck</title>",
    "src-tauri/src/lib.rs": [
      'menu_item("tray-app-name", "AgentDeck")',
      'menu_item("open", "Open AgentDeck")',
      "builder = builder.icon_as_template(true)",
    ].join("\n"),
    "src-tauri/src/commands/settings.rs": "# AgentDeck Diagnostics",
    "src/views/Settings.tsx": settingsSource(),
    "src/i18n/en.json": JSON.stringify({ app: { name: "AgentDeck" } }),
    "src/i18n/zh-TW.json": JSON.stringify({ app: { name: "AgentDeck" } }),
    "README.md": [
      '<h1 align="center">AgentDeck</h1>',
      "Legacy compatibility: ~/.skills-manager, skills-manager.db, and skills-manager-cli remain unchanged.",
      `Forked from ${UPSTREAM_REPO_URL}.`,
    ].join("\n"),
    "src-tauri/icons/icon-source.png": Buffer.from("agentdeck-icon-fixture"),
  };

  for (const asset of REQUIRED_DESKTOP_ASSETS) {
    files[asset] ??= Buffer.from("generated-asset");
  }

  Object.assign(files, overrides);
  for (const [relativePath, contents] of Object.entries(files)) {
    if (contents === null) continue;
    const target = path.join(rootDir, relativePath);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, contents);
  }

  return rootDir;
}

function runCliFixture(rootDir) {
  return spawnSync(
    process.execPath,
    [checkerPath, "--root", rootDir, "--upstream-icon-sha256", upstreamIconSha256],
    { encoding: "utf8" },
  );
}

function assertSingleViolation(overrides, expectedFile, expectedRule) {
  const rootDir = writeFixture(overrides);
  try {
    const violations = checkProductIdentity({ rootDir, upstreamIconSha256 });
    assert.equal(violations.length, 1, JSON.stringify(violations, null, 2));
    assert.equal(violations[0].file, expectedFile);
    assert.equal(violations[0].rule, expectedRule);

    const result = runCliFixture(rootDir);
    assert.equal(result.status, 1);
    assert.match(result.stderr, new RegExp(expectedFile.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.match(result.stderr, new RegExp(`\\[${expectedRule}\\]`));
  } finally {
    fs.rmSync(rootDir, { recursive: true, force: true });
  }
}

test("upstream Settings repository destination reports its file and rule", () => {
  assertSingleViolation(
    { "src/views/Settings.tsx": settingsSource({ repositoryBase: UPSTREAM_REPO_URL }) },
    "src/views/Settings.tsx",
    "repository-destination",
  );
});

test("Settings action bypassing the managed constant reports its file and rule", () => {
  assertSingleViolation(
    {
      "src/views/Settings.tsx": settingsSource({
        issueAction:
          'await openUrl("https://github.com/xingkongliang/skills-manager/issues/new?template=bug_report.md");',
      }),
    },
    "src/views/Settings.tsx",
    "repository-destination",
  );
});

test("Settings repository action pointing elsewhere reports its file and rule", () => {
  assertSingleViolation(
    { "src/views/Settings.tsx": settingsSource({ repositoryAction: 'await openUrl(DOCS_URL);' }) },
    "src/views/Settings.tsx",
    "repository-destination",
  );
});

test("valid identity and explicit legacy compatibility exceptions pass", () => {
  const rootDir = writeFixture();
  try {
    assert.deepEqual(checkProductIdentity({ rootDir, upstreamIconSha256 }), []);
    assert.equal(runCliFixture(rootDir).status, 0);
  } finally {
    fs.rmSync(rootDir, { recursive: true, force: true });
  }
});

test("wrong display name reports its file and rule", () => {
  assertSingleViolation(
    { "index.html": "<title>Skills Manager</title>" },
    "index.html",
    "display-name",
  );
});

test("wrong Bundle ID reports its file and rule", () => {
  assertSingleViolation(
    {
      "src-tauri/tauri.conf.json": JSON.stringify({
        productName: "AgentDeck",
        identifier: "com.example.agentdeck",
        app: { windows: [{ title: "AgentDeck" }] },
      }),
    },
    "src-tauri/tauri.conf.json",
    "bundle-id",
  );
});

test("wrong desktop package reports its file and rule", () => {
  assertSingleViolation(
    { "package.json": JSON.stringify({ name: "skills-manager" }) },
    "package.json",
    "desktop-package",
  );
});

test("upstream icon hash reports its file and rule", () => {
  assertSingleViolation(
    { "src-tauri/icons/icon-source.png": upstreamIcon },
    "src-tauri/icons/icon-source.png",
    "independent-icon",
  );
});

test("missing desktop asset reports its file and rule", () => {
  assertSingleViolation(
    { "src-tauri/icons/icon.icns": null },
    "src-tauri/icons/icon.icns",
    "required-asset",
  );
});

test("in-app logo left on upstream artwork reports its file and rule", () => {
  assertSingleViolation(
    { "public/icons/32x32.png": Buffer.from("upstream-sidebar-logo") },
    "public/icons/32x32.png",
    "in-app-logo",
  );
});
