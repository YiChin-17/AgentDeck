import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  checkPullRequestValidation,
  pullRequestPathFilters,
} from "./check-pull-request-validation.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(root, "scripts", "check-pull-request-validation.mjs");

const WORKFLOW = ".github/workflows/test.yml";
const PACKAGE = "package.json";

/// The fixture workflow is written the way the committed workflow is written, so
/// the contract is asserted against a realistic pull-request pipeline rather
/// than against a list of the checker's own tokens.
const CLEAN_WORKFLOW = `name: Test

on:
  push:
    branches: [main]
    paths:
      - 'src-tauri/**'
      - '.github/workflows/test.yml'
  pull_request:

permissions:
  contents: read

jobs:
  node-validation:
    runs-on: ubuntu-22.04
    name: Node validation

    steps:
      - name: Checkout repository
        uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - name: Setup Node.js
        uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: 22
          cache: npm

      - name: Install frontend dependencies
        run: npm ci

      - name: Frontend production build
        run: npm run build

      - name: Lint
        run: npm run lint

      - name: Locale integrity
        run: npm run check:i18n

      - name: Repository Node contracts
        run: node --test scripts/*.test.mjs

  rust-tests:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest
            label: macOS
            allow-failure: false
          - platform: windows-latest
            label: Windows
            allow-failure: true

    runs-on: \${{ matrix.platform }}
    name: Rust tests (\${{ matrix.label }})

    steps:
      - name: Checkout repository
        uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - name: Run tests
        continue-on-error: \${{ matrix.allow-failure }}
        run: cargo test --manifest-path src-tauri/Cargo.toml

  linux-check:
    runs-on: ubuntu-22.04
    name: Rust check (Linux)

    steps:
      - name: Checkout repository
        uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - name: Check
        run: cargo check --manifest-path src-tauri/Cargo.toml
`;

const CLEAN_PACKAGE = JSON.stringify(
  {
    name: "agentdeck",
    version: "1.30.0",
    scripts: {
      build: "tsc -b && vite build",
      lint: "eslint .",
      "check:i18n": "node scripts/check-i18n-locales.mjs",
      "check:pull-request-validation": "node scripts/check-pull-request-validation.mjs",
    },
  },
  null,
  2,
);

/// Every command the release regression job already runs for the frontend and
/// repository surface. A pull request that changes those surfaces has to meet
/// the same gates, so the checker and these fixtures name the same list.
const REQUIRED_COMMANDS = [
  { rule: "dependency-install", command: "npm ci" },
  { rule: "frontend-build", command: "npm run build" },
  { rule: "lint", command: "npm run lint" },
  { rule: "locale-integrity", command: "npm run check:i18n" },
  { rule: "node-contracts", command: "node --test scripts/*.test.mjs" },
];

function defaultFixture() {
  return {
    [WORKFLOW]: CLEAN_WORKFLOW,
    [PACKAGE]: CLEAN_PACKAGE,
  };
}

function withFixture(mutate, callback) {
  const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-pr-validation-"));
  try {
    const files = defaultFixture();
    mutate?.(files);
    for (const [relativePath, contents] of Object.entries(files)) {
      if (contents === null) continue;
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, contents, "utf8");
    }
    return callback(fixtureRoot);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

function findingsFor(mutate) {
  return withFixture(mutate, (fixtureRoot) => {
    const { failures } = checkPullRequestValidation({ rootDir: fixtureRoot });
    return {
      codes: [...new Set(failures.map((failure) => failure.code))].sort(),
      locations: [...new Set(failures.map((failure) => failure.location))].sort(),
      messages: failures.map((failure) => failure.message).join(" | "),
    };
  });
}

function runFixture(mutate) {
  return withFixture(mutate, (fixtureRoot) =>
    spawnSync(process.execPath, [checkerPath, "--root", fixtureRoot], { encoding: "utf8" }),
  );
}

function editWorkflow(files, replace, replacement = "") {
  const next = files[WORKFLOW].replace(replace, replacement);
  assert.notEqual(next, files[WORKFLOW], `fixture mutation did not change the workflow: ${replace}`);
  files[WORKFLOW] = next;
}

test("clean pull-request fixture reports the stable success summary", () => {
  const result = runFixture();

  assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
  assert.equal(
    result.stdout.trim(),
    "Pull-request validation contract passed: trigger=unfiltered node-commands=5 rust=macOS,Windows,Linux",
  );
});

test("a restrictive pull_request path filter is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(
      files,
      "  pull_request:\n",
      "  pull_request:\n    paths:\n      - 'src-tauri/**'\n      - '.github/workflows/test.yml'\n",
    );
  });

  assert.deepEqual(findings.codes, ["pull_request_paths_filter"]);
  assert.deepEqual(findings.locations, [WORKFLOW]);
  assert.match(findings.messages, /src-tauri/);
});

test("a paths-ignore filter on pull_request is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(files, "  pull_request:\n", "  pull_request:\n    paths-ignore:\n      - 'docs/**'\n");
  });

  assert.deepEqual(findings.codes, ["pull_request_paths_filter"]);
  assert.match(findings.messages, /paths-ignore/);
});

test("a missing pull_request trigger is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(files, "  pull_request:\n", "");
  });

  assert.ok(findings.codes.includes("pull_request_trigger_missing"), findings.messages);
  assert.deepEqual(findings.locations, [WORKFLOW]);
});

for (const { rule, command } of REQUIRED_COMMANDS) {
  test(`a workflow fixture without \`${command}\` reports the missing-command rule`, () => {
    const result = runFixture((files) => {
      editWorkflow(files, new RegExp(`^ +run: ${command.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\n`, "m"), "");
    });
    const output = `${result.stdout}${result.stderr}`;

    assert.notEqual(result.status, 0);
    assert.match(output, /node_command_missing/);
    assert.match(output, new RegExp(rule));
    assert.match(output, /\.github\/workflows\/test\.yml/);
  });
}

test("a missing Node validation job is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(files, /^ {2}node-validation:\n[\s\S]*?(?=^ {2}rust-tests:)/m, "");
  });

  assert.ok(findings.codes.includes("node_job_missing"), findings.messages);
  assert.deepEqual(findings.locations, [WORKFLOW]);
});

test("an unpinned Node setup action is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(
      files,
      "uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4",
      "uses: actions/setup-node@v4",
    );
  });

  assert.deepEqual(findings.codes, ["node_setup_unpinned"]);
});

test("a Node version other than 22 is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(files, "node-version: 22", "node-version: 20");
  });

  assert.deepEqual(findings.codes, ["node_version_mismatch"]);
  assert.match(findings.messages, /22/);
});

test("suppressing a Node command failure is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(
      files,
      "      - name: Lint\n        run: npm run lint",
      "      - name: Lint\n        continue-on-error: true\n        run: npm run lint",
    );
  });

  assert.deepEqual(findings.codes, ["node_command_suppressed"]);
  assert.match(findings.messages, /continue-on-error/);
});

test("making a Node command conditional is a finding", () => {
  const findings = findingsFor((files) => {
    editWorkflow(
      files,
      "      - name: Locale integrity\n        run: npm run check:i18n",
      "      - name: Locale integrity\n        if: github.event_name == 'push'\n        run: npm run check:i18n",
    );
  });

  assert.deepEqual(findings.codes, ["node_command_suppressed"]);
  assert.match(findings.messages, /if:/);
});

const RUST_PLATFORMS = [
  {
    label: "macOS",
    mutate: (files) => editWorkflow(files, "          - platform: macos-latest\n            label: macOS\n            allow-failure: false\n"),
  },
  {
    label: "Windows",
    mutate: (files) => editWorkflow(files, "          - platform: windows-latest\n            label: Windows\n            allow-failure: true\n"),
  },
  {
    label: "Linux",
    mutate: (files) => editWorkflow(files, /^ {2}linux-check:\n[\s\S]*$/m, ""),
  },
];

for (const platform of RUST_PLATFORMS) {
  test(`removing the ${platform.label} Rust coverage is a finding`, () => {
    const findings = findingsFor(platform.mutate);

    assert.ok(findings.codes.includes("rust_coverage_missing"), findings.messages);
    assert.match(findings.messages, new RegExp(platform.label));
  });
}

test("a package without the checker script is a finding", () => {
  const findings = findingsFor((files) => {
    const manifest = JSON.parse(files[PACKAGE]);
    delete manifest.scripts["check:pull-request-validation"];
    files[PACKAGE] = JSON.stringify(manifest, null, 2);
  });

  assert.deepEqual(findings.codes, ["package_script_missing"]);
  assert.deepEqual(findings.locations, [PACKAGE]);
});

test("the committed workflow and package script satisfy the contract", () => {
  const { failures } = checkPullRequestValidation({ rootDir: root });

  assert.deepEqual(
    failures.map((failure) => `[${failure.code}] ${failure.location}: ${failure.message}`),
    [],
  );
});

/// The pull-request trigger is what decides whether a change is validated at
/// all, so the surfaces named by the spec are asserted against the committed
/// trigger rather than against the fixture.
test("the committed pull-request trigger excludes no changed-file surface", () => {
  const workflowText = fs.readFileSync(path.join(root, WORKFLOW), "utf8");
  const filters = pullRequestPathFilters(workflowText);

  assert.deepEqual(filters, [], "pull requests must not be filtered by changed paths");

  for (const surface of [
    "src/App.tsx",
    "src/locales/en/translation.json",
    "scripts/check-pull-request-validation.mjs",
    "package.json",
    ".github/workflows/test.yml",
  ]) {
    assert.equal(
      filters.length,
      0,
      `a path filter would decide whether ${surface} triggers the Test workflow`,
    );
  }
});

test("the committed Node job runs every required command as a blocking step", () => {
  const workflowText = fs.readFileSync(path.join(root, WORKFLOW), "utf8");
  // Stops at the next job key or at the end of the file; `$` alone would end
  // the slice at the first line break under the multiline flag.
  const nodeJob =
    workflowText.match(/^ {2}node-validation:\n([\s\S]*?)(?=^ {2}[A-Za-z0-9_-]+:\s*$|$(?![\s\S]))/m)?.[1] ?? "";

  assert.notEqual(nodeJob, "", "the committed workflow has no node-validation job");
  for (const { command } of REQUIRED_COMMANDS) {
    assert.match(
      nodeJob,
      new RegExp(`^ +run: ${command.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`, "m"),
      `the node-validation job does not run ${command}`,
    );
  }
  assert.doesNotMatch(nodeJob, /continue-on-error/);
  assert.doesNotMatch(nodeJob, /^\s+if:/m);
});

test("the committed workflow keeps the existing Rust platform coverage", () => {
  const workflowText = fs.readFileSync(path.join(root, WORKFLOW), "utf8");

  assert.match(workflowText, /^ {2}rust-tests:$/m);
  assert.match(workflowText, /platform: macos-latest/);
  assert.match(workflowText, /platform: windows-latest/);
  assert.match(workflowText, /^ {2}linux-check:$/m);
  assert.match(workflowText, /cargo test --manifest-path src-tauri\/Cargo\.toml/);
  assert.match(workflowText, /cargo check --manifest-path src-tauri\/Cargo\.toml/);
});

test("the checker reports every failure with a rule and a file", () => {
  const result = runFixture((files) => {
    editWorkflow(files, "      - name: Lint\n        run: npm run lint\n\n", "");
    const manifest = JSON.parse(files[PACKAGE]);
    delete manifest.scripts["check:pull-request-validation"];
    files[PACKAGE] = JSON.stringify(manifest, null, 2);
  });
  const output = `${result.stdout}${result.stderr}`;

  assert.notEqual(result.status, 0);
  assert.match(output, /\[node_command_missing\] \.github\/workflows\/test\.yml:/);
  assert.match(output, /\[package_script_missing\] package\.json:/);
});

test("the checker reads committed files without running commands", () => {
  const source = fs.readFileSync(checkerPath, "utf8");

  assert.doesNotMatch(source, /node:child_process|execFileSync|spawnSync|execSync/);
});
