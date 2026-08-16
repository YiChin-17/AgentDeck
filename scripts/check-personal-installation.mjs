#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { findViolations } from "./check-no-upstream-app-updater.mjs";

const EXPECTED_BUNDLE_ID = "io.github.yichin17.agentdeck";
const EXPECTED_PRODUCT_NAME = "AgentDeck";
const EXPECTED_APP_NAME = "AgentDeck.app";

/// The bundle root is fixed on purpose. A checker that accepts an arbitrary
/// application path would let an unrelated build pass Phase 7 acceptance.
const BUNDLE_ROOT = "src-tauri/target/release/bundle";
const APP_PATH = `${BUNDLE_ROOT}/macos/${EXPECTED_APP_NAME}`;
const INSTALLER_DIR = `${BUNDLE_ROOT}/dmg`;
const INSTALLER_EXTENSION = ".dmg";

/// Topics a reader must find before a local build counts as installable. The
/// patterns describe what the text has to tell the reader, not how it is worded.
const REQUIRED_DOCUMENTATION = [
  { id: "locked-build", pattern: /npm ci[\s\S]*npm run tauri:build/ },
  { id: "install-location", pattern: /\/Applications/ },
  { id: "first-launch", pattern: /first launch/i },
  { id: "legacy-data-reuse", pattern: /\.skills-manager/ },
  { id: "library-offline", pattern: /Library Offline/i },
  { id: "backup-and-restore", pattern: /back(?:s|ed)? ?up[\s\S]*restore/i },
  { id: "uninstall-keeps-data", pattern: /does not remove your data/i },
  {
    id: "no-auto-update",
    pattern: /no application auto-update[\s\S]*?no notarization guarantee/i,
  },
];

/// Claims the personal-installation section must never carry: release trust does
/// not transfer to a local build. These are scoped to that section because the
/// same README also documents the official signed release, where the very same
/// sentences are true.
const FORBIDDEN_PERSONAL_CLAIMS = [
  {
    id: "upstream-signing-trust",
    pattern: /signed with an Apple Developer ID|notarized by Apple|公開 release 已簽章/i,
  },
];

/// No section of ours tells a reader to weaken macOS security, so this one is
/// checked across the whole document.
const FORBIDDEN_DOCUMENTATION = [
  {
    id: "gatekeeper-bypass",
    pattern: /xattr -cr|spctl --master-disable|disable Gatekeeper|停用 Gatekeeper/i,
  },
];

/// The personal-installation section runs from its own heading to the next
/// top-level section.
function personalInstallationSection(readme) {
  const start = readme.search(/^##\s+Personal installation/m);
  if (start === -1) return null;
  const rest = readme.slice(start);
  const end = rest.indexOf("\n## ", 1);
  return end === -1 ? rest : rest.slice(0, end);
}

function readText(rootDir, relativePath) {
  try {
    return fs.readFileSync(path.join(rootDir, relativePath), "utf8");
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

function readJson(rootDir, relativePath) {
  const text = readText(rootDir, relativePath);
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/// `plutil` is the macOS-native plist reader, so no parser of ours can drift
/// from what the system itself sees in the bundle.
function readInfoPlist(rootDir) {
  const plistPath = path.join(rootDir, APP_PATH, "Contents", "Info.plist");
  if (!fs.existsSync(plistPath)) return null;
  try {
    return JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", plistPath], {
      encoding: "utf8",
    }));
  } catch {
    return null;
  }
}

function listInstallers(rootDir) {
  try {
    return fs
      .readdirSync(path.join(rootDir, INSTALLER_DIR))
      .filter((entry) => entry.endsWith(INSTALLER_EXTENSION))
      .sort();
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
}

function isExecutableFile(filePath) {
  try {
    const stats = fs.statSync(filePath);
    return stats.isFile() && (stats.mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

function checkDocumentation(rootDir, fail) {
  const readme = readText(rootDir, "README.md");
  if (readme === null) {
    fail("documentation_incomplete", "README.md", "file is missing");
    return;
  }

  const section = personalInstallationSection(readme);
  if (section === null) {
    fail("documentation_incomplete", "README.md", "personal installation section is missing");
    return;
  }

  const missing = REQUIRED_DOCUMENTATION.filter((topic) => !topic.pattern.test(section));
  if (missing.length > 0) {
    fail(
      "documentation_incomplete",
      "README.md",
      `missing required personal-installation topic(s): ${missing.map((topic) => topic.id).join(", ")}`,
    );
  }

  const claims = FORBIDDEN_PERSONAL_CLAIMS.filter((rule) => rule.pattern.test(section));
  const forbidden = [...claims, ...FORBIDDEN_DOCUMENTATION.filter((rule) => rule.pattern.test(readme))];
  if (forbidden.length > 0) {
    fail(
      "documentation_incomplete",
      "README.md",
      `contains forbidden personal-installation claim(s) or instruction(s): ${forbidden.map((rule) => rule.id).join(", ")}`,
    );
  }
}

export function checkPersonalInstallation({ rootDir, platform = process.platform }) {
  const failures = [];
  const fail = (code, location, message) => failures.push({ code, location, message });

  if (platform !== "darwin") {
    fail(
      "unsupported_host",
      APP_PATH,
      `packaged inspection requires macOS, current platform is "${platform}"`,
    );
    return { failures, summary: null };
  }

  const tauriConfig = readJson(rootDir, "src-tauri/tauri.conf.json");
  const packageJson = readJson(rootDir, "package.json");
  const expectedVersion = tauriConfig?.version ?? null;

  if (!expectedVersion) {
    fail("identity_mismatch", "src-tauri/tauri.conf.json", "committed bundle version is unreadable");
  } else if (packageJson?.version !== expectedVersion) {
    fail(
      "version_mismatch",
      "package.json",
      `version "${packageJson?.version}" differs from src-tauri/tauri.conf.json "${expectedVersion}"`,
    );
  }

  const info = readInfoPlist(rootDir);
  if (info === null) {
    fail("bundle_missing", APP_PATH, "application bundle or its Info.plist is not present");
  } else {
    const identifier = info.CFBundleIdentifier;
    const bundleName = info.CFBundleName ?? info.CFBundleDisplayName;
    if (identifier !== EXPECTED_BUNDLE_ID || bundleName !== EXPECTED_PRODUCT_NAME) {
      fail(
        "identity_mismatch",
        APP_PATH,
        `bundle reports name "${bundleName}" and identifier "${identifier}", expected "${EXPECTED_PRODUCT_NAME}" and "${EXPECTED_BUNDLE_ID}"`,
      );
    }

    const bundleVersions = [info.CFBundleShortVersionString, info.CFBundleVersion];
    if (expectedVersion && bundleVersions.some((value) => value !== expectedVersion)) {
      fail(
        "version_mismatch",
        APP_PATH,
        `bundle reports version ${bundleVersions.map((value) => `"${value}"`).join(" / ")}, expected "${expectedVersion}"`,
      );
    }

    const executableName = info.CFBundleExecutable;
    const executablePath = path.join(rootDir, APP_PATH, "Contents", "MacOS", executableName ?? "");
    if (!executableName || !isExecutableFile(executablePath)) {
      fail(
        "executable_missing",
        `${APP_PATH}/Contents/MacOS/${executableName ?? ""}`,
        "main executable is absent or not executable",
      );
    }
  }

  const installers = listInstallers(rootDir);
  if (installers.length === 0) {
    fail("installer_missing", INSTALLER_DIR, `no ${INSTALLER_EXTENSION} installer for this build`);
  } else {
    const named = installers.filter((entry) => entry.startsWith(`${EXPECTED_PRODUCT_NAME}_`));
    if (named.length === 0) {
      fail(
        "identity_mismatch",
        INSTALLER_DIR,
        `installer(s) ${installers.join(", ")} do not package ${EXPECTED_PRODUCT_NAME}`,
      );
    } else if (expectedVersion && !named.some((entry) => entry.includes(`_${expectedVersion}_`))) {
      fail(
        "version_mismatch",
        INSTALLER_DIR,
        `installer(s) ${named.join(", ")} do not carry version "${expectedVersion}"`,
      );
    }
  }

  let updaterViolations;
  try {
    updaterViolations = findViolations(rootDir);
  } catch (error) {
    updaterViolations = null;
    fail("updater_surface_present", BUNDLE_ROOT, `updater surface is unverifiable: ${error.message}`);
  }
  if (updaterViolations?.length) {
    const first = updaterViolations[0];
    fail(
      "updater_surface_present",
      first.file,
      `${updaterViolations.length} updater surface violation(s), first at line ${first.line} [${first.pattern}]`,
    );
  }

  checkDocumentation(rootDir, fail);

  const summary =
    failures.length === 0
      ? `Personal installation check passed: app=${EXPECTED_APP_NAME} identifier=${EXPECTED_BUNDLE_ID} version=${expectedVersion} updater=absent docs=complete`
      : null;
  return { failures, summary };
}

function runCli() {
  let rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (option !== "--root" || !value) {
      console.error(`Unknown or incomplete option: ${option ?? ""}`);
      process.exitCode = 2;
      return;
    }
    rootDir = path.resolve(value);
  }

  const { failures, summary } = checkPersonalInstallation({ rootDir });
  if (summary) {
    console.log(summary);
    return;
  }

  console.error("Personal installation check failed:");
  for (const failure of failures) {
    console.error(`- [${failure.code}] ${failure.location}: ${failure.message}`);
  }
  process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli();
}
