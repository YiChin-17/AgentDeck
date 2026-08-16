import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { SURFACE_PATHS } from "./check-no-upstream-app-updater.mjs";
import { checkPersonalInstallation } from "./check-personal-installation.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(root, "scripts", "check-personal-installation.mjs");

const BUNDLE_ID = "io.github.yichin17.agentdeck";
const VERSION = "1.30.0";
const BUNDLE_ROOT = "src-tauri/target/release/bundle";
const APP_DIR = `${BUNDLE_ROOT}/macos/AgentDeck.app`;

/// The fixture README is written as prose a reader would actually follow, not as
/// a list of the checker's own patterns — otherwise the documentation contract
/// would only be asserting against itself.
const COMPLETE_README = `# AgentDeck

## Personal installation (local build)

AgentDeck is a personal local build. Build it yourself from source with the
committed lockfiles:

\`\`\`bash
npm ci
npm run tauri:build
\`\`\`

The build writes \`src-tauri/target/release/bundle/macos/AgentDeck.app\` plus a
macOS installer under \`src-tauri/target/release/bundle/\`. Move
\`AgentDeck.app\` into \`/Applications\` (or your personal \`~/Applications\`
folder) to install it.

### First launch and existing data

On first launch AgentDeck reuses the existing \`.skills-manager\` library,
its SQLite database, presets, projects and Git backup metadata in place. Nothing
is renamed, moved or recreated, and the \`skills-manager-cli\` binary keeps its
current contract.

### Library location and offline recovery

An internal library lives inside the application data directory; an external
library stays wherever you configured it. When the configured external library
is unreachable the app shows Library Offline and changes nothing on disk — use
Retry once the volume is back.

### Git backup and restore

Connect a Git remote to back up the library, and restore from that same remote
on another machine or after a reinstall.

### Uninstall

Removing \`AgentDeck.app\` does not remove your data. The library, database and
Git backup credential are listed individually so you can delete only what you
choose to delete.

### No application auto-update

This personal build has no application auto-update, no public release hosting,
no Developer ID signing and no notarization guarantee.
`;

function plist(entries) {
  const body = Object.entries(entries)
    .map(([key, value]) => `\t<key>${key}</key>\n\t<string>${value}</string>`)
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
${body}
</dict>
</plist>
`;
}

function defaultFixture() {
  const files = {};
  for (const relativePath of SURFACE_PATHS) files[relativePath] = "";
  files["package.json"] = JSON.stringify({ name: "agentdeck", version: VERSION }, null, 2);
  files["src-tauri/tauri.conf.json"] = JSON.stringify(
    { productName: "AgentDeck", version: VERSION, identifier: BUNDLE_ID },
    null,
    2,
  );
  files["README.md"] = COMPLETE_README;
  files[`${APP_DIR}/Contents/Info.plist`] = plist({
    CFBundleName: "AgentDeck",
    CFBundleDisplayName: "AgentDeck",
    CFBundleIdentifier: BUNDLE_ID,
    CFBundleExecutable: "AgentDeck",
    CFBundleShortVersionString: VERSION,
    CFBundleVersion: VERSION,
  });
  files[`${APP_DIR}/Contents/MacOS/AgentDeck`] = "#!/bin/sh\nexit 0\n";
  files[`${BUNDLE_ROOT}/dmg/AgentDeck_${VERSION}_aarch64.dmg`] = "fixture installer";
  return files;
}

const EXECUTABLE_FILES = new Set([`${APP_DIR}/Contents/MacOS/AgentDeck`]);

function withFixture(mutate, callback) {
  const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-personal-install-"));
  try {
    const files = defaultFixture();
    mutate?.(files);
    for (const [relativePath, contents] of Object.entries(files)) {
      if (contents === null) continue;
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, contents, "utf8");
      if (EXECUTABLE_FILES.has(relativePath)) fs.chmodSync(filePath, 0o755);
    }
    return callback(fixtureRoot);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

function runFixture(mutate) {
  return withFixture(mutate, (fixtureRoot) =>
    spawnSync(process.execPath, [checkerPath, "--root", fixtureRoot], { encoding: "utf8" }),
  );
}

function codesFor(mutate) {
  return withFixture(mutate, (fixtureRoot) =>
    checkPersonalInstallation({ rootDir: fixtureRoot, platform: "darwin" }).failures.map(
      (failure) => failure.code,
    ),
  );
}

test("clean AgentDeck bundle reports the stable success summary", () => {
  const result = runFixture();

  assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
  assert.equal(
    result.stdout.trim(),
    `Personal installation check passed: app=AgentDeck.app identifier=${BUNDLE_ID} version=${VERSION} updater=absent docs=complete`,
  );
});

test("missing application bundle reports bundle_missing", () => {
  const codes = codesFor((files) => {
    files[`${APP_DIR}/Contents/Info.plist`] = null;
    files[`${APP_DIR}/Contents/MacOS/AgentDeck`] = null;
  });

  assert.deepEqual(codes, ["bundle_missing"]);
});

test("missing macOS installer reports installer_missing", () => {
  const codes = codesFor((files) => {
    files[`${BUNDLE_ROOT}/dmg/AgentDeck_${VERSION}_aarch64.dmg`] = null;
  });

  assert.deepEqual(codes, ["installer_missing"]);
});

test("wrong Bundle ID reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    files[`${APP_DIR}/Contents/Info.plist`] = plist({
      CFBundleName: "AgentDeck",
      CFBundleIdentifier: "io.example.other",
      CFBundleExecutable: "AgentDeck",
      CFBundleShortVersionString: VERSION,
      CFBundleVersion: VERSION,
    });
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("wrong bundle display name reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    files[`${APP_DIR}/Contents/Info.plist`] = plist({
      CFBundleName: "Skills Manager",
      CFBundleIdentifier: BUNDLE_ID,
      CFBundleExecutable: "AgentDeck",
      CFBundleShortVersionString: VERSION,
      CFBundleVersion: VERSION,
    });
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("installer built for another product reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    files[`${BUNDLE_ROOT}/dmg/AgentDeck_${VERSION}_aarch64.dmg`] = null;
    files[`${BUNDLE_ROOT}/dmg/SkillsManager_${VERSION}_aarch64.dmg`] = "fixture installer";
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("bundle version differing from committed configuration reports version_mismatch", () => {
  const codes = codesFor((files) => {
    files[`${APP_DIR}/Contents/Info.plist`] = plist({
      CFBundleName: "AgentDeck",
      CFBundleIdentifier: BUNDLE_ID,
      CFBundleExecutable: "AgentDeck",
      CFBundleShortVersionString: "1.29.0",
      CFBundleVersion: "1.29.0",
    });
  });

  assert.deepEqual(codes, ["version_mismatch"]);
});

test("package version differing from Tauri configuration reports version_mismatch", () => {
  const codes = codesFor((files) => {
    files["package.json"] = JSON.stringify({ name: "agentdeck", version: "1.31.0" }, null, 2);
  });

  assert.deepEqual(codes, ["version_mismatch"]);
});

test("installer version differing from committed configuration reports version_mismatch", () => {
  const codes = codesFor((files) => {
    files[`${BUNDLE_ROOT}/dmg/AgentDeck_${VERSION}_aarch64.dmg`] = null;
    files[`${BUNDLE_ROOT}/dmg/AgentDeck_1.29.0_aarch64.dmg`] = "fixture installer";
  });

  assert.deepEqual(codes, ["version_mismatch"]);
});

test("absent main executable reports executable_missing", () => {
  const codes = codesFor((files) => {
    files[`${APP_DIR}/Contents/MacOS/AgentDeck`] = null;
  });

  assert.deepEqual(codes, ["executable_missing"]);
});

test("non-executable main binary reports executable_missing", () => {
  const result = withFixture(null, (fixtureRoot) => {
    fs.chmodSync(path.join(fixtureRoot, APP_DIR, "Contents/MacOS/AgentDeck"), 0o644);
    return checkPersonalInstallation({ rootDir: fixtureRoot, platform: "darwin" });
  });

  assert.deepEqual(
    result.failures.map((failure) => failure.code),
    ["executable_missing"],
  );
});

test("updater build surface reports updater_surface_present", () => {
  const codes = codesFor((files) => {
    files["src-tauri/tauri.conf.json"] = JSON.stringify(
      {
        productName: "AgentDeck",
        version: VERSION,
        identifier: BUNDLE_ID,
        pubkey: "fixture-key",
      },
      null,
      2,
    );
  });

  assert.deepEqual(codes, ["updater_surface_present"]);
});

test("documentation missing a required topic reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files["README.md"] = COMPLETE_README.replace(
      /### Uninstall[\s\S]*?### No application auto-update/,
      "### No application auto-update",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("documentation claiming upstream signing trust reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files["README.md"] = COMPLETE_README.replace(
      "### No application auto-update",
      "This build is signed with an Apple Developer ID certificate and notarized by Apple.\n\n### No application auto-update",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("an official release section may describe signing outside the personal section", () => {
  const codes = codesFor((files) => {
    files["README.md"] = `${COMPLETE_README}
## Official macOS release

Every published disk image is signed with an Apple Developer ID certificate and
notarized by Apple. See docs/macos-distribution.md for the verification steps.
`;
  });

  assert.deepEqual(codes, []);
});

test("a README without a personal installation section reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files["README.md"] = COMPLETE_README.replace(
      "## Personal installation (local build)",
      "## Getting started",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("documentation instructing a Gatekeeper bypass reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files["README.md"] = `${COMPLETE_README}\nIf the app will not open, run \`xattr -cr /Applications/AgentDeck.app\`.\n`;
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("packaged inspection on a non-macOS host reports unsupported_host only", () => {
  const result = withFixture(null, (fixtureRoot) =>
    checkPersonalInstallation({ rootDir: fixtureRoot, platform: "linux" }),
  );

  assert.deepEqual(
    result.failures.map((failure) => failure.code),
    ["unsupported_host"],
  );
});

test("failing check exits non-zero and names the stable code and artifact location", () => {
  const result = runFixture((files) => {
    files[`${APP_DIR}/Contents/Info.plist`] = null;
    files[`${APP_DIR}/Contents/MacOS/AgentDeck`] = null;
  });
  const output = `${result.stdout}${result.stderr}`;

  assert.notEqual(result.status, 0);
  assert.match(output, /bundle_missing/);
  assert.match(output, /src-tauri\/target\/release\/bundle\/macos\/AgentDeck\.app/);
});

const EVIDENCE_PATH = "docs/personal-installation-verification.md";
const EVIDENCE_SECTIONS = [
  "Environment",
  "Artifacts",
  "Automated checks",
  "Packaged smoke",
  "Data compatibility",
  "Warnings",
];

/// Evidence is committed, so anything machine-specific or secret in it leaks
/// permanently. Absolute home and temporary paths also make the record
/// unreproducible for anyone else building the same commit.
const SENSITIVE_EVIDENCE = [
  { id: "home-path", pattern: /\/Users\/|\/home\/|\$HOME|~\/(?!Applications)/ },
  { id: "temporary-path", pattern: /\/private\/var\/folders|\/var\/folders|(?<![\w./])\/tmp\// },
  { id: "credential", pattern: /ghp_[A-Za-z0-9]|sk-[A-Za-z0-9]|Bearer\s+[A-Za-z0-9._-]{8}|BEGIN [A-Z ]*PRIVATE KEY/ },
  { id: "keychain-secret", pattern: /password:\s*\S|"secret"\s*:/i },
];

test("the repository README satisfies the personal installation documentation contract", () => {
  const { failures } = checkPersonalInstallation({ rootDir: root, platform: "darwin" });

  assert.deepEqual(
    failures.filter((failure) => failure.code === "documentation_incomplete"),
    [],
  );
});

test("the verification evidence carries all six required sections", () => {
  const evidence = fs.readFileSync(path.join(root, EVIDENCE_PATH), "utf8");

  const missing = EVIDENCE_SECTIONS.filter(
    (section) => !new RegExp(`^##\\s+${section}\\s*$`, "m").test(evidence),
  );

  assert.deepEqual(missing, []);
});

test("the verification evidence records project-relative artifact paths only", () => {
  const evidence = fs.readFileSync(path.join(root, EVIDENCE_PATH), "utf8");

  const found = SENSITIVE_EVIDENCE.filter((rule) => rule.pattern.test(evidence)).map(
    (rule) => rule.id,
  );

  assert.deepEqual(found, []);
  assert.match(evidence, /src-tauri\/target\/release\/bundle\/macos\/AgentDeck\.app/);
});

test("checker reads a fixed bundle root without network, mutation, or home fallback", () => {
  const source = fs.readFileSync(checkerPath, "utf8");

  assert.doesNotMatch(source, /node:https?|\bfetch\(|XMLHttpRequest|https?:\/\//);
  assert.doesNotMatch(source, /os\.homedir|process\.env\.HOME|~\/(?!Applications)/);
  assert.doesNotMatch(source, /writeFile|mkdir|rm\(|rmSync|unlink|chmod|copyFile|rename/);
  assert.match(source, /src-tauri\/target\/release\/bundle/);
});
