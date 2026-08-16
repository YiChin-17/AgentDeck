import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { SURFACE_PATHS } from "./check-no-upstream-app-updater.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(root, "scripts", "check-no-upstream-app-updater.mjs");

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function withFixture(overrides, callback) {
  const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-updater-check-"));
  try {
    for (const relativePath of SURFACE_PATHS) {
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, overrides[relativePath] ?? "", "utf8");
    }
    for (const [relativePath, source] of Object.entries(overrides)) {
      if (SURFACE_PATHS.includes(relativePath)) continue;
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, source, "utf8");
    }
    callback(fixtureRoot);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

function runFixture(overrides) {
  let result;
  withFixture(overrides, (fixtureRoot) => {
    result = spawnSync(process.execPath, [checkerPath], {
      cwd: fixtureRoot,
      encoding: "utf8",
    });
  });
  return result;
}

test("AgentDeck does not check for application binary releases", () => {
  const forbidden = [
    ["src-tauri/src/commands/settings.rs", /check_app_update|AppUpdateInfo|skills-manager\/releases\/latest/],
    ["src-tauri/src/lib.rs", /commands::settings::check_app_update/],
    ["src/lib/tauri.ts", /checkAppUpdate|AppUpdateInfo|check_app_update/],
    ["src/context/AppContext.tsx", /appUpdate|refreshAppUpdate|checkAppUpdate|app-update-available/],
  ];

  const violations = forbidden.flatMap(([relativePath, pattern]) => {
    const match = read(relativePath).match(pattern);
    return match ? [`${relativePath}: ${match[0]}`] : [];
  });

  assert.deepEqual(violations, []);
});

test("AgentDeck cannot download or install application updates from Settings", () => {
  const settingsSource = read("src/views/Settings.tsx");
  const sidebarSource = read("src/components/Sidebar.tsx");
  const forbidden = [
    ["src/views/Settings.tsx", settingsSource, /@tauri-apps\/plugin-updater|checkUpdater|handleCheckUpdate|handleAutoUpdate\b|updateInstallBlocker|restartApp|appUpdate/],
    ["src/components/Sidebar.tsx", sidebarSource, /appUpdate|settings\.updateAvailable/],
  ];

  const violations = forbidden.flatMap(([relativePath, source, pattern]) => {
    const match = source.match(pattern);
    return match ? [`${relativePath}: ${match[0]}`] : [];
  });

  assert.deepEqual(violations, []);
});

test("packaged AgentDeck has no application updater build surface", () => {
  const forbidden = [
    ["src-tauri/src/lib.rs", /tauri_plugin_updater/],
    ["src-tauri/tauri.conf.json", /"updater"|createUpdaterArtifacts|"pubkey"|"endpoints"|skills-manager\/releases\/latest\/download/],
    ["src-tauri/capabilities/default.json", /updater:default/],
    ["package.json", /@tauri-apps\/plugin-updater/],
    ["package-lock.json", /@tauri-apps\/plugin-updater/],
    ["src-tauri/Cargo.toml", /tauri-plugin-updater/],
    ["src-tauri/Cargo.lock", /tauri-plugin-updater/],
  ];

  const violations = forbidden.flatMap(([relativePath, pattern]) => {
    const match = read(relativePath).match(pattern);
    return match ? [`${relativePath}: ${match[0]}`] : [];
  });

  assert.deepEqual(violations, []);
});

test("upstream provenance remains separate from runtime update trust", () => {
  const readme = read("README.md");
  const baseline = read("BASELINE.md");
  const license = read("LICENSE");

  assert.match(readme, /github\.com\/xingkongliang\/skills-manager/);
  assert.match(readme, /MIT License/);
  assert.match(baseline, /upstream.*github\.com\/xingkongliang\/skills-manager\.git/s);
  assert.match(license, /^MIT License/);
  assert.doesNotMatch(readme, /In-app updates|installs it for you on macOS and Windows/);
});

test("the personal installation guide claims no upstream release trust", () => {
  const readme = read("README.md");
  // Scoped to the personal-installation section: the same README now also
  // documents the official signed release, where Developer ID and notarization
  // language is accurate. The bundle a reader builds here is unsigned and
  // unnotarized, so that language would transfer a trust chain the local build
  // does not have.
  const start = readme.search(/^##\s+Personal installation/m);
  assert.notEqual(start, -1);
  const rest = readme.slice(start);
  const end = rest.indexOf("\n## ", 1);
  const section = end === -1 ? rest : rest.slice(0, end);

  assert.doesNotMatch(section, /signed with an Apple Developer ID/);
  assert.doesNotMatch(section, /notarized by Apple/);
  // Telling any reader to strip quarantine attributes or turn off Gatekeeper
  // would ask them to pay for a trust gap with their own system security, so
  // this stays a whole-document rule.
  assert.doesNotMatch(readme, /xattr -cr/);
  assert.doesNotMatch(readme, /spctl --master-disable/);

  assert.match(section, /personal local build|personal installation/i);
  assert.match(section, /no application auto-update/i);
  assert.match(section, /no notarization guarantee/i);
});

test("official distribution documentation may describe the signed release", () => {
  const guide = read("docs/macos-distribution.md");

  assert.match(guide, /GitHub Release/i);
  assert.match(guide, /Developer ID Application/);
  assert.match(guide, /notariz/i);
  // Hosting a signed download is not an update feed: nothing here may point the
  // application at a manifest, a signature or an archive it could install.
  assert.doesNotMatch(guide, /latest\.json|\.sig\b|\.app\.tar\.gz|releases\/latest\/download/);
});

test("Phase 7 documents personal installation without app auto-update", () => {
  const plan = read("plan.md");
  const phase7 = plan.match(/### Phase 7：[\s\S]*?(?=\n## )/)?.[0] ?? "";

  assert.match(phase7, /穩定化與個人安裝/);
  assert.match(phase7, /本機.*build/i);
  assert.match(phase7, /資料備份/);
  assert.match(phase7, /解除安裝/);
  assert.match(phase7, /公開 release/);
  assert.match(phase7, /distribution signing/);
  assert.match(phase7, /notarization/);
  assert.match(phase7, /auto-update/);
  assert.match(phase7, /新的 Spectra change/);
});

test("clean fixture passes the updater regression CLI", () => {
  const result = runFixture({});

  assert.equal(result.status, 0, result.stderr);
});

const violationFixtures = [
  {
    name: "release query",
    file: "src-tauri/src/commands/settings.rs",
    source: 'client.get("https://api.github.com/repos/xingkongliang/skills-manager/releases/latest");',
    pattern: "upstream-release-query",
  },
  {
    name: "plugin registration",
    file: "src-tauri/src/lib.rs",
    source: ".plugin(tauri_plugin_updater::Builder::new().build())",
    pattern: "tauri-updater-registration",
  },
  {
    name: "permission",
    file: "src-tauri/capabilities/default.json",
    source: '{ "permissions": ["updater:default"] }',
    pattern: "tauri-updater-permission",
  },
  {
    name: "endpoint",
    file: "src-tauri/tauri.conf.json",
    source: '{ "endpoints": ["https://example.test/latest.json"] }',
    pattern: "tauri-updater-endpoint",
  },
  {
    name: "pubkey",
    file: "src-tauri/tauri.conf.json",
    source: '{ "pubkey": "test-key" }',
    pattern: "tauri-updater-pubkey",
  },
  {
    name: "JavaScript dependency",
    file: "package.json",
    source: '{ "dependencies": { "@tauri-apps/plugin-updater": "2" } }',
    pattern: "javascript-updater-dependency",
  },
  {
    name: "Rust dependency",
    file: "src-tauri/Cargo.toml",
    source: 'tauri-plugin-updater = "2"',
    pattern: "rust-updater-dependency",
  },
  {
    name: "frontend install flow",
    file: "src/views/Settings.tsx",
    source: "await update.downloadAndInstall();",
    pattern: "frontend-updater-install-flow",
  },
  {
    name: "documented update manifest",
    file: "docs/macos-distribution.md",
    source: "Point the app at https://example.test/latest.json for updates.",
    pattern: "distribution-update-manifest",
  },
  {
    name: "documented updater archive",
    file: "README.md",
    source: "Download AgentDeck_1.31.0_aarch64.app.tar.gz to update in place.",
    pattern: "distribution-update-archive",
  },
  {
    name: "release workflow updater key",
    file: ".github/workflows/release.yml",
    source: "  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
    pattern: "distribution-updater-key",
  },
];

for (const fixture of violationFixtures) {
  test(`${fixture.name} fixture fails and reports its location`, () => {
    const result = runFixture({ [fixture.file]: fixture.source });
    const output = `${result.stdout}${result.stderr}`;

    assert.notEqual(result.status, 0);
    assert.match(output, new RegExp(fixture.file.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.match(output, new RegExp(fixture.pattern));
  });
}

test("official release documentation fixture passes", () => {
  const result = runFixture({
    "docs/macos-distribution.md":
      "Every GitHub Release asset is signed with a Developer ID Application certificate and notarized by Apple.",
  });

  assert.equal(result.status, 0, result.stderr);
});

test("attribution fixture passes because documents are outside runtime surfaces", () => {
  const result = runFixture({
    "README.md": "Forked from https://github.com/xingkongliang/skills-manager/releases/latest",
    "BASELINE.md": "upstream: https://github.com/xingkongliang/skills-manager.git",
    "LICENSE": "MIT License",
  });

  assert.equal(result.status, 0, result.stderr);
});
