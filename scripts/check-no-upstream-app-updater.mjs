#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const RUNTIME_FILES = [
  "src-tauri/src/commands/settings.rs",
  "src-tauri/src/lib.rs",
  "src/lib/tauri.ts",
  "src/context/AppContext.tsx",
  "src/views/Settings.tsx",
  "src/components/Sidebar.tsx",
];

const RULES = [
  {
    id: "upstream-release-query",
    paths: RUNTIME_FILES,
    pattern: /(?:api\.github\.com\/repos|github\.com)\/(?:xingkongliang\/skills-manager|YiChin-17\/AgentDeck)\/releases/i,
  },
  {
    id: "app-update-ipc",
    paths: RUNTIME_FILES,
    pattern: /check_app_update|checkAppUpdate|AppUpdateInfo/,
  },
  {
    id: "app-update-context",
    paths: ["src/context/AppContext.tsx", "src/views/Settings.tsx", "src/components/Sidebar.tsx"],
    pattern: /\bappUpdate\b|\brefreshAppUpdate\b|app-update-available/,
  },
  {
    id: "frontend-updater-install-flow",
    paths: ["src/views/Settings.tsx", "src/lib/tauri.ts"],
    pattern: /@tauri-apps\/plugin-updater|downloadAndInstall|updateInstallBlocker|restartApp|app-update-restart/,
  },
  {
    id: "tauri-updater-registration",
    paths: ["src-tauri/src/lib.rs"],
    pattern: /tauri_plugin_updater/,
  },
  {
    id: "tauri-updater-permission",
    paths: ["src-tauri/capabilities/default.json"],
    pattern: /updater:[a-z-]+/,
  },
  {
    id: "tauri-updater-config",
    paths: ["src-tauri/tauri.conf.json"],
    pattern: /"updater"\s*:/,
  },
  {
    id: "tauri-updater-endpoint",
    paths: ["src-tauri/tauri.conf.json"],
    pattern: /"endpoints"\s*:|\/releases\/latest\/download\/latest\.json/i,
  },
  {
    id: "tauri-updater-pubkey",
    paths: ["src-tauri/tauri.conf.json"],
    pattern: /"pubkey"\s*:/,
  },
  {
    id: "tauri-updater-artifacts",
    paths: ["src-tauri/tauri.conf.json"],
    pattern: /"createUpdaterArtifacts"\s*:/,
  },
  {
    id: "javascript-updater-dependency",
    paths: ["package.json", "package-lock.json"],
    pattern: /@tauri-apps\/plugin-updater/,
  },
  {
    id: "rust-updater-dependency",
    paths: ["src-tauri/Cargo.toml", "src-tauri/Cargo.lock"],
    pattern: /tauri-plugin-updater/,
  },
];

export const SURFACE_PATHS = [...new Set(RULES.flatMap((rule) => rule.paths))];

function lineNumber(source, offset) {
  return source.slice(0, offset).split("\n").length;
}

export function findViolations(root = process.cwd()) {
  const sources = new Map(
    SURFACE_PATHS.map((relativePath) => [
      relativePath,
      fs.readFileSync(path.join(root, relativePath), "utf8"),
    ]),
  );
  const violations = [];

  for (const rule of RULES) {
    for (const relativePath of rule.paths) {
      const source = sources.get(relativePath);
      const flags = rule.pattern.flags.includes("g")
        ? rule.pattern.flags
        : `${rule.pattern.flags}g`;
      const pattern = new RegExp(rule.pattern.source, flags);
      for (const match of source.matchAll(pattern)) {
        violations.push({
          file: relativePath,
          line: lineNumber(source, match.index),
          pattern: rule.id,
          match: match[0],
        });
      }
    }
  }

  return violations;
}

export function runCheck(root = process.cwd()) {
  let violations;
  try {
    violations = findViolations(root);
  } catch (error) {
    console.error(`App updater source check could not read a checked surface: ${error.message}`);
    return 1;
  }

  if (violations.length === 0) {
    console.log("App updater source check passed.");
    return 0;
  }

  console.error(`App updater source check failed with ${violations.length} violation(s):`);
  for (const violation of violations) {
    console.error(
      `  ${violation.file}:${violation.line} [${violation.pattern}] ${violation.match}`,
    );
  }
  return 1;
}

const isCli = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isCli) {
  process.exitCode = runCheck();
}
