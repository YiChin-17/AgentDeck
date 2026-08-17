import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const EXPECTED_BUNDLE_ID = "io.github.yichin17.agentdeck";
const UPSTREAM_ICON_SHA256 = "7a4602adb9dc8bedb51da6e9c6293007b5d577a24827e9d2ec37b4f5e0f50090";

/// The sidebar logo the webview loads. It is a separate copy of the 32 px
/// desktop output, so regenerating the icons alone leaves it on the previous
/// artwork — which is exactly how the upstream logo survived the rename.
const IN_APP_LOGO = "public/icons/32x32.png";
const DESKTOP_32PX_ICON = "src-tauri/icons/32x32.png";

export const REQUIRED_DESKTOP_ASSETS = [
  "assets/icon.png",
  IN_APP_LOGO,
  "src-tauri/icons/icon.png",
  "src-tauri/icons/32x32.png",
  "src-tauri/icons/64x64.png",
  "src-tauri/icons/128x128.png",
  "src-tauri/icons/128x128@2x.png",
  "src-tauri/icons/icon.icns",
  "src-tauri/icons/icon.ico",
  "src-tauri/icons/StoreLogo.png",
  "src-tauri/icons/Square30x30Logo.png",
  "src-tauri/icons/Square44x44Logo.png",
  "src-tauri/icons/Square71x71Logo.png",
  "src-tauri/icons/Square89x89Logo.png",
  "src-tauri/icons/Square107x107Logo.png",
  "src-tauri/icons/Square142x142Logo.png",
  "src-tauri/icons/Square150x150Logo.png",
  "src-tauri/icons/Square284x284Logo.png",
  "src-tauri/icons/Square310x310Logo.png",
  "src-tauri/icons/tray/tray-icon-source.png",
  "src-tauri/icons/tray/tray-icon-16.png",
  "src-tauri/icons/tray/tray-icon-20.png",
  "src-tauri/icons/tray/tray-icon-24.png",
  "src-tauri/icons/tray/tray-icon-32.png",
];

/// Both Settings actions share one repository constant, so the fork's identity
/// survives only if the constant holds the AgentDeck repo AND each action still
/// derives its destination from it — a hardcoded URL in one action would
/// silently keep sending users upstream.
const SETTINGS_SOURCE = "src/views/Settings.tsx";
const AGENTDECK_REPO_URL = "https://github.com/YiChin-17/AgentDeck";
const UPSTREAM_REPO_URL = "https://github.com/xingkongliang/skills-manager";

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const SETTINGS_REPOSITORY_DESTINATIONS = [
  new RegExp(`const GITHUB_URL = "${escapeRegExp(AGENTDECK_REPO_URL)}";`),
  /openUrl\(GITHUB_URL\)/,
  /openUrl\(`\$\{GITHUB_URL\}\/issues\/new\?template=bug_report\.md`\)/,
];

const DISPLAY_SURFACES = [
  ["index.html", /<title>AgentDeck<\/title>/],
  ["src-tauri/src/lib.rs", /Open AgentDeck/],
  ["src-tauri/src/commands/settings.rs", /# AgentDeck Diagnostics/],
  ["src/views/Settings.tsx", /auto-collected by AgentDeck/],
  ["README.md", /<h1 align="center">AgentDeck<\/h1>/],
];

const LEGACY_DISPLAY_ALLOWLIST = new Map([
  ["README.md", [/Skills Manager\.app/g]],
]);

function addViolation(violations, file, rule, message) {
  violations.push({ file, rule, message });
}

function readFile(rootDir, relativePath) {
  try {
    return fs.readFileSync(path.join(rootDir, relativePath));
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

function parseJson(rootDir, relativePath, violations) {
  const buffer = readFile(rootDir, relativePath);
  if (!buffer) {
    addViolation(violations, relativePath, "required-surface", "file is missing");
    return null;
  }
  try {
    return JSON.parse(buffer.toString("utf8"));
  } catch (error) {
    addViolation(violations, relativePath, "required-surface", `invalid JSON: ${error.message}`);
    return null;
  }
}

function containsUnallowedDisplayName(relativePath, text) {
  let candidate = text;
  for (const pattern of LEGACY_DISPLAY_ALLOWLIST.get(relativePath) ?? []) {
    candidate = candidate.replace(pattern, "");
  }
  return candidate.includes("Skills Manager");
}

function collectStrings(value, result = []) {
  if (typeof value === "string") result.push(value);
  if (Array.isArray(value)) {
    for (const item of value) collectStrings(item, result);
  } else if (value && typeof value === "object") {
    for (const item of Object.values(value)) collectStrings(item, result);
  }
  return result;
}

function checkSettingsRepositoryDestinations(rootDir, violations) {
  const text = readFile(rootDir, SETTINGS_SOURCE)?.toString("utf8") ?? null;
  /// A missing file is already reported by the display-surface loop.
  if (text === null) return;
  const usesManagedDestinations = SETTINGS_REPOSITORY_DESTINATIONS.every((pattern) =>
    pattern.test(text),
  );
  if (!usesManagedDestinations || text.includes(UPSTREAM_REPO_URL)) {
    addViolation(
      violations,
      SETTINGS_SOURCE,
      "repository-destination",
      `repository and report-issue actions must derive from the managed ${AGENTDECK_REPO_URL} constant and must not target ${UPSTREAM_REPO_URL}`,
    );
  }
}

function readPackageName(cargoToml, section) {
  const escapedSection = escapeRegExp(section);
  const match = cargoToml.match(new RegExp(`^\\[${escapedSection}\\]\\s*([\\s\\S]*?)(?=^\\[|\\Z)`, "m"));
  return match?.[1].match(/^name\s*=\s*"([^"]+)"/m)?.[1] ?? null;
}

export function checkProductIdentity({
  rootDir,
  upstreamIconSha256 = UPSTREAM_ICON_SHA256,
}) {
  const violations = [];
  const packageJson = parseJson(rootDir, "package.json", violations);
  const packageLock = parseJson(rootDir, "package-lock.json", violations);
  const tauri = parseJson(rootDir, "src-tauri/tauri.conf.json", violations);

  if (packageJson && packageJson.name !== "agentdeck") {
    addViolation(violations, "package.json", "desktop-package", 'name must be "agentdeck"');
  }
  if (
    packageLock &&
    (packageLock.name !== "agentdeck" || packageLock.packages?.[""]?.name !== "agentdeck")
  ) {
    addViolation(
      violations,
      "package-lock.json",
      "desktop-package",
      'root package names must be "agentdeck"',
    );
  }
  if (tauri?.productName !== "AgentDeck" || tauri?.app?.windows?.[0]?.title !== "AgentDeck") {
    addViolation(
      violations,
      "src-tauri/tauri.conf.json",
      "display-name",
      'productName and main window title must be "AgentDeck"',
    );
  }
  if (tauri && tauri.identifier !== EXPECTED_BUNDLE_ID) {
    addViolation(
      violations,
      "src-tauri/tauri.conf.json",
      "bundle-id",
      `identifier must be "${EXPECTED_BUNDLE_ID}"`,
    );
  }

  const cargoPath = "src-tauri/Cargo.toml";
  const cargo = readFile(rootDir, cargoPath)?.toString("utf8") ?? null;
  if (cargo === null) {
    addViolation(violations, cargoPath, "required-surface", "file is missing");
  } else if (
    readPackageName(cargo, "package") !== "agentdeck" ||
    !/^default-run\s*=\s*"agentdeck"$/m.test(cargo)
  ) {
    addViolation(
      violations,
      cargoPath,
      "desktop-package",
      'package name and default-run must be "agentdeck"',
    );
  }

  for (const [relativePath, expected] of DISPLAY_SURFACES) {
    const text = readFile(rootDir, relativePath)?.toString("utf8") ?? null;
    if (text === null) {
      addViolation(violations, relativePath, "required-surface", "file is missing");
    } else if (!expected.test(text) || containsUnallowedDisplayName(relativePath, text)) {
      addViolation(
        violations,
        relativePath,
        "display-name",
        "checked product surface must identify AgentDeck and contain no unallowlisted Skills Manager display name",
      );
    }
  }

  checkSettingsRepositoryDestinations(rootDir, violations);

  for (const relativePath of ["src/i18n/en.json", "src/i18n/zh-TW.json"]) {
    const locale = parseJson(rootDir, relativePath, violations);
    if (
      locale &&
      (locale.app?.name !== "AgentDeck" || collectStrings(locale).some((value) => value.includes("Skills Manager")))
    ) {
      addViolation(
        violations,
        relativePath,
        "display-name",
        'App-owned locale strings must use "AgentDeck"',
      );
    }
  }

  const masterPath = "src-tauri/icons/icon-source.png";
  const master = readFile(rootDir, masterPath);
  if (!master?.length) {
    addViolation(violations, masterPath, "required-asset", "icon master is missing or empty");
  } else {
    const sha256 = crypto.createHash("sha256").update(master).digest("hex");
    if (sha256 === upstreamIconSha256) {
      addViolation(violations, masterPath, "independent-icon", "icon master matches upstream artwork");
    }
  }

  for (const relativePath of REQUIRED_DESKTOP_ASSETS) {
    if (!readFile(rootDir, relativePath)?.length) {
      addViolation(violations, relativePath, "required-asset", "generated asset is missing or empty");
    }
  }

  const inAppLogo = readFile(rootDir, IN_APP_LOGO);
  const desktop32px = readFile(rootDir, DESKTOP_32PX_ICON);
  if (inAppLogo?.length && desktop32px?.length && !inAppLogo.equals(desktop32px)) {
    addViolation(
      violations,
      IN_APP_LOGO,
      "in-app-logo",
      `must be byte-identical to ${DESKTOP_32PX_ICON}`,
    );
  }

  return violations;
}

function runCli() {
  let rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  let upstreamIconSha256 = UPSTREAM_ICON_SHA256;
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!value || !["--root", "--upstream-icon-sha256"].includes(option)) {
      console.error(`Unknown or incomplete option: ${option ?? ""}`);
      process.exitCode = 2;
      return;
    }
    if (option === "--root") rootDir = path.resolve(value);
    if (option === "--upstream-icon-sha256") upstreamIconSha256 = value;
  }

  const violations = checkProductIdentity({ rootDir, upstreamIconSha256 });
  if (violations.length === 0) {
    console.log("Product identity check passed.");
    return;
  }

  console.error("Product identity check failed:");
  for (const violation of violations) {
    console.error(`- ${violation.file} [${violation.rule}]: ${violation.message}`);
  }
  process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli();
}
