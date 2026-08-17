#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/// This checker proves the *shape* of the committed pull-request pipeline: that
/// no change can reach main without passing the frontend and repository gates
/// the release regression job already runs. GitHub decides which workflows run
/// from the committed trigger, so nothing here talks to the network or to the
/// Actions API — it only reads committed files.

const WORKFLOW = ".github/workflows/test.yml";
const PACKAGE = "package.json";

const NODE_JOB = "node-validation";
const RUST_TEST_JOB = "rust-tests";
const LINUX_JOB = "linux-check";
const NODE_VERSION = "22";
const PACKAGE_SCRIPT = "check:pull-request-validation";
const CHECKER_SCRIPT = "scripts/check-pull-request-validation.mjs";

/// The frontend and repository subset of the release regression job. Both
/// validation paths name the same list so a command cannot be dropped from one
/// and survive in the other.
const REQUIRED_COMMANDS = [
  { rule: "dependency-install", command: "npm ci" },
  { rule: "frontend-build", command: "npm run build" },
  { rule: "lint", command: "npm run lint" },
  { rule: "locale-integrity", command: "npm run check:i18n" },
  { rule: "node-contracts", command: "node --test scripts/*.test.mjs" },
];

/// Rust coverage that predates this contract. Node validation is added beside
/// it, never in place of it.
const RUST_PLATFORMS = [
  { label: "macOS", job: RUST_TEST_JOB, pattern: /platform: macos-latest/ },
  { label: "Windows", job: RUST_TEST_JOB, pattern: /platform: windows-latest/ },
  { label: "Linux", job: LINUX_JOB, pattern: /cargo check/ },
];

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function readText(rootDir, relativePath) {
  try {
    return fs.readFileSync(path.join(rootDir, relativePath), "utf8");
  } catch {
    return null;
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

function indentOf(line) {
  return line.length - line.trimStart().length;
}

/// A GitHub workflow is a small, regular subset of YAML. The rules here only
/// need to know where a key's block starts and ends, so slicing blocks by
/// indentation keeps the checker on the standard library instead of taking a
/// YAML dependency for one file.
function findSection(lines, key, indent) {
  const header = new RegExp(`^ {${indent}}${escapeRegExp(key)}:(?:\\s+(.*?))?\\s*$`);

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(header);
    if (!match) continue;

    const body = [];
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const line = lines[cursor];
      if (line.trim() === "" || line.trim().startsWith("#")) {
        body.push(line);
        continue;
      }
      if (indentOf(line) <= indent) break;
      body.push(line);
    }
    return { inline: (match[1] ?? "").trim(), body };
  }

  return null;
}

function sequenceItems(section) {
  if (!section) return [];
  if (section.inline.startsWith("[") && section.inline.endsWith("]")) {
    const inner = section.inline.slice(1, -1).trim();
    return inner === "" ? [] : inner.split(",").map((item) => item.trim());
  }
  return section.body
    .map((line) => line.trim())
    .filter((line) => line.startsWith("- "))
    .map((line) => line.slice(2).trim());
}

/// The changed-path filters GitHub would apply to a pull request. `null` means
/// the workflow declares no pull_request trigger at all; an empty array means
/// every pull request enters the workflow.
export function pullRequestPathFilters(workflowText) {
  const trigger = findSection(workflowText.split("\n"), "on", 0);
  if (!trigger) return null;

  const pullRequest = findSection(trigger.body, "pull_request", 2);
  if (!pullRequest) return null;

  const filters = [];
  for (const key of ["paths", "paths-ignore"]) {
    const section = findSection(pullRequest.body, key, 4);
    for (const item of sequenceItems(section)) {
      filters.push(`${key}: ${item}`);
    }
  }
  return filters;
}

function jobText(workflowText, name) {
  const jobs = findSection(workflowText.split("\n"), "jobs", 0);
  if (!jobs) return null;
  const job = findSection(jobs.body, name, 2);
  return job ? job.body.join("\n") : null;
}

function checkTrigger(workflowText, fail) {
  const filters = pullRequestPathFilters(workflowText);

  if (filters === null) {
    fail(
      "pull_request_trigger_missing",
      WORKFLOW,
      "the workflow declares no pull_request trigger, so pull requests are never validated",
    );
    return;
  }
  if (filters.length > 0) {
    fail(
      "pull_request_paths_filter",
      WORKFLOW,
      `pull requests are filtered by changed paths (${filters.join(", ")}), so a change outside the filter is never validated`,
    );
  }
}

function checkNodeJob(workflowText, fail) {
  const job = jobText(workflowText, NODE_JOB);

  if (job === null) {
    fail("node_job_missing", WORKFLOW, `the workflow has no ${NODE_JOB} job`);
    return;
  }

  if (!/uses:\s*actions\/setup-node@[0-9a-f]{40}\b/.test(job)) {
    fail(
      "node_setup_unpinned",
      WORKFLOW,
      `the ${NODE_JOB} job does not pin actions/setup-node to a commit SHA`,
    );
  }
  if (!new RegExp(`node-version:\\s*"?${NODE_VERSION}"?\\s*$`, "m").test(job)) {
    fail(
      "node_version_mismatch",
      WORKFLOW,
      `the ${NODE_JOB} job must run Node ${NODE_VERSION}, matching the release regression job`,
    );
  }

  for (const { rule, command } of REQUIRED_COMMANDS) {
    if (!new RegExp(`^\\s*run: ${escapeRegExp(command)}\\s*$`, "m").test(job)) {
      fail(
        "node_command_missing",
        WORKFLOW,
        `the ${NODE_JOB} job is missing the required command "${command}" (${rule})`,
      );
    }
  }

  // A required gate that can be skipped or that reports success after failing
  // is not a gate, so neither suppression form is allowed anywhere in the job.
  if (/continue-on-error/.test(job)) {
    fail(
      "node_command_suppressed",
      WORKFLOW,
      `the ${NODE_JOB} job uses continue-on-error, which hides a failed required command`,
    );
  }
  if (/^\s*if:/m.test(job)) {
    fail(
      "node_command_suppressed",
      WORKFLOW,
      `the ${NODE_JOB} job uses an "if:" condition, which can skip a required command`,
    );
  }
}

function checkRustCoverage(workflowText, fail) {
  for (const platform of RUST_PLATFORMS) {
    const job = jobText(workflowText, platform.job);
    if (job !== null && platform.pattern.test(job)) continue;
    fail(
      "rust_coverage_missing",
      WORKFLOW,
      `the workflow no longer runs the ${platform.label} Rust checks (${platform.job})`,
    );
  }
}

function checkPackageScript(rootDir, fail) {
  const manifest = readJson(rootDir, PACKAGE);

  if (manifest === null) {
    fail("package_script_missing", PACKAGE, "package.json is missing or is not valid JSON");
    return;
  }

  const script = manifest.scripts?.[PACKAGE_SCRIPT];
  if (typeof script !== "string") {
    fail("package_script_missing", PACKAGE, `package.json does not expose the ${PACKAGE_SCRIPT} script`);
    return;
  }
  if (!script.includes(CHECKER_SCRIPT)) {
    fail(
      "package_script_missing",
      PACKAGE,
      `the ${PACKAGE_SCRIPT} script does not run ${CHECKER_SCRIPT}`,
    );
  }
}

export function checkPullRequestValidation({ rootDir }) {
  const failures = [];
  const fail = (code, location, message) => failures.push({ code, location, message });

  const workflowText = readText(rootDir, WORKFLOW);
  if (workflowText === null) {
    fail("workflow_missing", WORKFLOW, "the pull-request test workflow is missing");
    checkPackageScript(rootDir, fail);
    return { failures, summary: null };
  }

  checkTrigger(workflowText, fail);
  checkNodeJob(workflowText, fail);
  checkRustCoverage(workflowText, fail);
  checkPackageScript(rootDir, fail);

  const summary =
    failures.length === 0
      ? `Pull-request validation contract passed: trigger=unfiltered node-commands=${REQUIRED_COMMANDS.length} rust=${RUST_PLATFORMS.map(
          ({ label }) => label,
        ).join(",")}`
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

  const { failures, summary } = checkPullRequestValidation({ rootDir });
  if (summary) {
    console.log(summary);
    return;
  }

  console.error("Pull-request validation contract failed:");
  for (const failure of failures) {
    console.error(`- [${failure.code}] ${failure.location}: ${failure.message}`);
  }
  process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli();
}
