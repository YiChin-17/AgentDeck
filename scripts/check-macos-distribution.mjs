#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/// This checker proves the *shape* of the committed release pipeline: that a tag
/// push cannot reach a public release without passing every macOS trust gate.
/// Live signing and notarization can only be exercised inside the protected
/// release environment, so nothing here talks to GitHub, Apple, the network or
/// any local credential store — it only reads committed files.

const EXPECTED_PRODUCT_NAME = "AgentDeck";
const EXPECTED_BUNDLE_ID = "io.github.yichin17.agentdeck";
const RELEASE_ENVIRONMENT = "macos-release";
const BUILD_JOB = "build";
const PUBLISH_JOB = "publish";
const REGRESSION_JOB = "regression";

const RELEASE_WORKFLOW = ".github/workflows/release.yml";
const DISTRIBUTION_DOC = "docs/macos-distribution.md";
const READme = "README.md";

const EXPECTED_TARGETS = [
  { arch: "arm64", target: "aarch64-apple-darwin", dmgArch: "aarch64" },
  { arch: "x86_64", target: "x86_64-apple-darwin", dmgArch: "x64" },
];

/// Identity tokens the release workflow has to carry. A release that builds,
/// names or verifies anything other than AgentDeck is not this product's release.
const IDENTITY_TOKENS = [
  { id: "verified-app-bundle", pattern: /AgentDeck\.app/ },
  { id: "release-disk-image-name", pattern: /AgentDeck_\$\{VERSION\}_/ },
  { id: "release-title", pattern: /--title "AgentDeck /, },
  { id: "expected-bundle-id", pattern: new RegExp(EXPECTED_BUNDLE_ID.replace(/\./g, "\\.")) },
];

const LEGACY_IDENTITY = /Skills[ _]?Manager|skills[-_]manager/i;

/// Gates that keep one tag, one commit and one version bound together. They live
/// in the build job because nothing may be signed before they pass.
const VERSION_GATES = [
  { id: "tag-shape", pattern: /v\[0-9\]\*\.\[0-9\]\*\.\[0-9\]\*\)/ },
  { id: "tag-version", pattern: /\$\{GITHUB_REF_NAME#v\}/ },
  { id: "package-version", pattern: /require\('\.\/package\.json'\)\.version/ },
  { id: "bundle-config-version", pattern: /require\('\.\/src-tauri\/tauri\.conf\.json'\)\.version/ },
  { id: "protected-history", pattern: /merge-base --is-ancestor/ },
  { id: "embedded-app-version", pattern: /CFBundleShortVersionString/ },
];

/// Apple trust gates. Each one answers a different question, so a missing gate
/// leaves a real hole: a valid signature from another team, an unstapled ticket,
/// or a disk image whose contents were never opened and checked.
const VERIFICATION_GATES = [
  { id: "strict-signature", pattern: /codesign --verify --deep --strict/ },
  { id: "developer-id-authority", pattern: /Authority=Developer ID Application:/ },
  { id: "expected-team", pattern: /TeamIdentifier=\$APPLE_TEAM_ID/ },
  { id: "secure-timestamp", pattern: /Timestamp=/ },
  { id: "hardened-runtime", pattern: /flags=\[\^\)\]\*runtime/ },
  { id: "stapled-ticket", pattern: /xcrun stapler validate/ },
  { id: "gatekeeper-assessment", pattern: /spctl --assess --type execute/ },
  { id: "disk-image-mount", pattern: /hdiutil attach/ },
  { id: "read-only-mount", pattern: /-readonly/ },
  { id: "disk-image-unmount", pattern: /hdiutil detach/ },
  { id: "unique-mounted-app", pattern: /maxdepth 1 -name '\*\.app'/ },
  { id: "embedded-bundle-id", pattern: /CFBundleIdentifier/ },
];

/// The Phase 7 gates that must still pass at release time. Signing proves who
/// built the artifact, not that the artifact works, so publication also waits on
/// the regressions that decide whether this commit is releasable at all.
const REGRESSION_GATES = [
  { id: "frontend-build", pattern: /npm run build/ },
  { id: "lint", pattern: /npm run lint/ },
  { id: "locale-integrity", pattern: /npm run check:i18n/ },
  { id: "node-contracts", pattern: /node --test scripts\/\*\.test\.mjs/ },
  { id: "rust-tests", pattern: /cargo test --locked/ },
  { id: "personal-installation", pattern: /npm run check:personal-installation/ },
];

const CHECKSUM_GATES = [
  { job: BUILD_JOB, id: "digest-generated", pattern: /shasum -a 256 /, },
  { job: BUILD_JOB, id: "digest-file", pattern: /\.sha256/ },
  { job: BUILD_JOB, id: "digest-self-check", pattern: /shasum -a 256 -c/ },
  { job: PUBLISH_JOB, id: "digest-reverified", pattern: /shasum -a 256 -c/ },
];

/// Anything that would turn a hosted release back into an application updater
/// feed. The release publishes disk images and checksums for manual download.
const UPDATER_ARTIFACTS = [
  { id: "update-manifest", pattern: /latest\.json/ },
  { id: "update-signature", pattern: /\.sig\b/ },
  { id: "update-archive", pattern: /\.app\.tar\.gz/ },
  { id: "updater-signing-key", pattern: /TAURI_SIGNING/ },
  { id: "updater-manifest-option", pattern: /updaterJsonKeepUniversal/ },
  { id: "updater-bundle-option", pattern: /createUpdaterArtifacts/ },
  { id: "updater-bundle-target", pattern: /--bundles [^\n]*updater/ },
  { id: "release-uploading-action", pattern: /tauri-apps\/tauri-action/ },
];

/// Ways a credential value escapes the job that holds it: printed to the log,
/// dumped from the ephemeral store, or carried out in a workflow artifact.
const SECRET_EXPOSURE = [
  { id: "logged-secret-expression", pattern: /echo\s+["']?\$\{\{\s*secrets\./ },
  { id: "logged-credential-value", pattern: /echo\s+["']?\$\{?APPLE_(CERTIFICATE|CERTIFICATE_PASSWORD|API_KEY_BASE64)/ },
  { id: "dumped-environment", pattern: /printenv/ },
  { id: "dumped-credential-store", pattern: /security dump-keychain/ },
  { id: "printed-private-key", pattern: /cat\s+["']?\$\{?(KEY_PATH|APPLE_API_KEY_PATH|CERT_PATH)/ },
];

const EXPORTED_CREDENTIAL_PATH = /private_keys|\.keychain|runner\.temp|RUNNER_TEMP/i;

const CREDENTIAL_MATERIAL = [
  { id: "private-key", pattern: /BEGIN [A-Z ]*PRIVATE KEY/ },
  { id: "access-token", pattern: /ghp_[A-Za-z0-9]|github_pat_[A-Za-z0-9]/ },
  { id: "api-key", pattern: /sk-[A-Za-z0-9]{8}/ },
];

/// Topics a downloader needs before an official artifact can be trusted: which
/// file to take, how to check it, what established the trust, and what happens
/// if a release is withdrawn.
const REQUIRED_DOC_TOPICS = [
  { id: "hosted-release", pattern: /GitHub Release/i },
  { id: "apple-silicon-download", pattern: /_aarch64\.dmg/ },
  { id: "intel-download", pattern: /_x64\.dmg/ },
  { id: "checksum-command", pattern: /shasum -a 256/ },
  { id: "checksum-asset", pattern: /\.sha256/ },
  { id: "developer-id", pattern: /Developer ID Application/ },
  { id: "notarization", pattern: /notariz/i },
  { id: "gatekeeper", pattern: /Gatekeeper/ },
  { id: "no-auto-update", pattern: /no application auto-update/i },
  { id: "withdrawal", pattern: /withdraw/i },
];

const FORBIDDEN_DOC_CONTENT = [
  { id: "gatekeeper-bypass", pattern: /xattr -cr|spctl --master-disable|disable Gatekeeper|停用 Gatekeeper/i },
  { id: "machine-specific-path", pattern: /\/Users\/|\/home\/|\$HOME/ },
];

const REQUIRED_README_TOPICS = [
  { id: "official-distribution-guide", pattern: /docs\/macos-distribution\.md/ },
  { id: "personal-build-disclaimer", pattern: /no application auto-update/i },
  { id: "distribution-inactive", pattern: /no public AgentDeck release/i },
];

/// AgentDeck is maintained for its owner only, so nothing is published. The
/// distribution material is kept as dormant instructions, and a reader has to
/// learn that before any download step, not in a footnote.
const DISTRIBUTION_INACTIVE = /no public AgentDeck release/i;

/// A dormant guide may explain how a release would be verified, but it must not
/// assert that one exists. Rather than banning words, each sentence that claims
/// artifacts are published must carry its own condition — which is exactly what
/// separates "when a version is published, ..." from "versions are published".
const PUBLICATION_CLAIM =
  /\b(?:is|are)\s+published\b|\bpublishes\b|\bofficial (?:AgentDeck )?download\b|\breleases? (?:is|are) available\b/i;
const CONDITIONAL_CLAIM =
  /\b(?:if|when|once|would|will|future|dormant|inactive|currently no|no public)\b/i;

function leadingBlock(text) {
  return text.split(/^## /m)[0];
}

function unconditionalPublicationClaims(text) {
  return text
    .split(/\n+|(?<=[.!?;])\s+/)
    .map((sentence) => sentence.trim())
    .filter((sentence) => PUBLICATION_CLAIM.test(sentence) && !CONDITIONAL_CLAIM.test(sentence));
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

/// A GitHub workflow is a small, regular subset of YAML: block maps, block
/// sequences and block scalars. Parsing that subset here keeps the checker on the
/// standard library instead of taking a YAML dependency for one file.
function parseScalar(raw) {
  const value = raw.trim();
  if (value.startsWith("[") && value.endsWith("]")) {
    const inner = value.slice(1, -1).trim();
    return inner === "" ? [] : inner.split(",").map((item) => parseScalar(item));
  }
  if (value.length >= 2 && ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'")))) {
    return value.slice(1, -1);
  }
  return value;
}

function indentOf(line) {
  return line.length - line.trimStart().length;
}

function isBlank(line) {
  return /^\s*(#.*)?$/.test(line);
}

function parseBlockScalar(lines, state, parentIndent) {
  const body = [];
  while (state.index < lines.length) {
    const line = lines[state.index];
    if (line.trim() !== "" && indentOf(line) <= parentIndent) break;
    body.push(line.trim() === "" ? "" : line.slice(parentIndent + 2));
    state.index += 1;
  }
  return body.join("\n");
}

function parseBlock(lines, state, indent) {
  let result = null;

  while (state.index < lines.length) {
    const line = lines[state.index];
    if (isBlank(line)) {
      state.index += 1;
      continue;
    }
    const currentIndent = indentOf(line);
    if (currentIndent < indent) break;

    const content = line.trim();
    if (content === "-" || content.startsWith("- ")) {
      if (result === null) result = [];
      if (!Array.isArray(result)) break;
      const itemText = content === "-" ? "" : content.slice(2);
      if (itemText === "") {
        state.index += 1;
        result.push(parseBlock(lines, state, currentIndent + 2));
      } else if (/^[^\s:#][^:]*:(\s|$)/.test(itemText)) {
        // A sequence item that starts a map: re-indent the line so the map body
        // and the item's remaining keys parse as one block.
        lines[state.index] = `${" ".repeat(currentIndent + 2)}${itemText}`;
        result.push(parseBlock(lines, state, currentIndent + 2));
      } else {
        state.index += 1;
        result.push(parseScalar(itemText));
      }
      continue;
    }

    const match = content.match(/^([^\s:#][^:]*):(?:\s+(.*))?$/);
    if (!match) {
      state.index += 1;
      continue;
    }
    if (Array.isArray(result)) break;
    if (result === null) result = {};

    const [, key, rawValue] = match;
    state.index += 1;
    if (rawValue === undefined || rawValue === "") {
      result[key] = parseBlock(lines, state, currentIndent + 1);
    } else if (/^[|>][+-]?$/.test(rawValue.trim())) {
      result[key] = parseBlockScalar(lines, state, currentIndent);
    } else {
      result[key] = parseScalar(rawValue);
    }
  }

  return result ?? {};
}

function parseWorkflow(text) {
  return parseBlock(text.split("\n"), { index: 0 }, 0);
}

/// A shell command split across continuation lines is one command, so the token
/// rules read the joined form. Otherwise a gate could disappear from a command
/// simply by being reformatted.
function joinContinuations(text) {
  return text.replace(/\\\n\s*/g, " ");
}

/// Raw per-job text, so token rules can say "this gate is missing from the build
/// job" instead of "this token is missing from the file".
function sliceJobs(text) {
  const jobs = new Map();
  let inJobs = false;
  let current = null;
  let buffer = [];

  const store = () => {
    if (current) jobs.set(current, joinContinuations(buffer.join("\n")));
  };

  for (const line of text.split("\n")) {
    if (/^jobs:\s*$/.test(line)) {
      inJobs = true;
      continue;
    }
    if (!inJobs) continue;
    if (line.trim() !== "" && indentOf(line) === 0) {
      store();
      current = null;
      inJobs = false;
      continue;
    }
    const header = line.match(/^ {2}([A-Za-z0-9_-]+):\s*$/);
    if (header) {
      store();
      current = header[1];
      buffer = [];
      continue;
    }
    if (current) buffer.push(line);
  }
  store();

  return jobs;
}

function permissionOf(node) {
  const contents = node?.permissions?.contents;
  return typeof contents === "string" ? contents : null;
}

function stepsOf(job) {
  return Array.isArray(job?.steps) ? job.steps : [];
}

function matrixTargets(job) {
  const include = job?.strategy?.matrix?.include;
  if (!Array.isArray(include)) return [];
  return include.map((entry) => entry?.target).filter((value) => typeof value === "string");
}

function checkIdentity(workflowText, tauriConfig, fail) {
  if (tauriConfig?.productName !== EXPECTED_PRODUCT_NAME) {
    fail(
      "identity_mismatch",
      "src-tauri/tauri.conf.json",
      `productName must be "${EXPECTED_PRODUCT_NAME}"`,
    );
  }
  if (tauriConfig?.identifier !== EXPECTED_BUNDLE_ID) {
    fail(
      "identity_mismatch",
      "src-tauri/tauri.conf.json",
      `identifier must be "${EXPECTED_BUNDLE_ID}"`,
    );
  }

  if (LEGACY_IDENTITY.test(workflowText)) {
    fail(
      "identity_mismatch",
      RELEASE_WORKFLOW,
      "release workflow still names the upstream Skills Manager product",
    );
  }
  const missing = IDENTITY_TOKENS.filter((token) => !token.pattern.test(workflowText));
  if (missing.length > 0) {
    fail(
      "identity_mismatch",
      RELEASE_WORKFLOW,
      `release workflow does not bind the AgentDeck identity: ${missing.map((token) => token.id).join(", ")}`,
    );
  }
}

function checkVersionBinding(packageJson, tauriConfig, buildText, fail) {
  const packageVersion = packageJson?.version ?? null;
  const bundleVersion = tauriConfig?.version ?? null;
  if (!packageVersion || !bundleVersion || packageVersion !== bundleVersion) {
    fail(
      "tag_version_mismatch",
      "package.json",
      `package version "${packageVersion}" differs from src-tauri/tauri.conf.json "${bundleVersion}"`,
    );
  }

  const missing = VERSION_GATES.filter((gate) => !gate.pattern.test(buildText));
  if (missing.length > 0) {
    fail(
      "tag_version_mismatch",
      RELEASE_WORKFLOW,
      `build job does not bind tag, commit and version: ${missing.map((gate) => gate.id).join(", ")}`,
    );
  }
}

function checkAuthority(workflow, jobs, fail) {
  if (permissionOf(workflow) !== "read") {
    fail(
      "release_authority_too_broad",
      RELEASE_WORKFLOW,
      "workflow-level permissions must be contents: read",
    );
  }

  for (const [name, job] of Object.entries(workflow?.jobs ?? {})) {
    const contents = permissionOf(job);
    if (name === PUBLISH_JOB) {
      if (contents !== "write") {
        fail(
          "release_authority_too_broad",
          RELEASE_WORKFLOW,
          `job "${name}" must declare contents: write as the only release writer`,
        );
      }
      continue;
    }
    if (contents !== "read") {
      fail(
        "release_authority_too_broad",
        RELEASE_WORKFLOW,
        `job "${name}" must declare contents: read`,
      );
    }
    if (/gh release (create|edit|upload|delete)/.test(jobs.get(name) ?? "")) {
      fail(
        "release_authority_too_broad",
        RELEASE_WORKFLOW,
        `job "${name}" must not create or modify a release`,
      );
    }
  }
}

function checkEnvironment(buildJob, fail) {
  if (buildJob?.environment !== RELEASE_ENVIRONMENT) {
    fail(
      "release_environment_missing",
      RELEASE_WORKFLOW,
      `job "${BUILD_JOB}" must run in the protected "${RELEASE_ENVIRONMENT}" environment`,
    );
  }
}

function checkSecretBoundary(workflowText, workflow, jobs, buildJob, buildText, fail) {
  for (const [name, text] of jobs) {
    if (name === BUILD_JOB) continue;
    if (/secrets\.APPLE_/.test(text)) {
      fail(
        "secret_boundary_violation",
        RELEASE_WORKFLOW,
        `job "${name}" reads Apple credentials outside the protected build job`,
      );
    }
  }

  const exposures = SECRET_EXPOSURE.filter((rule) => rule.pattern.test(workflowText));
  if (exposures.length > 0) {
    fail(
      "secret_boundary_violation",
      RELEASE_WORKFLOW,
      `release workflow can emit a credential value: ${exposures.map((rule) => rule.id).join(", ")}`,
    );
  }

  for (const [name, job] of Object.entries(workflow?.jobs ?? {})) {
    for (const step of stepsOf(job)) {
      if (typeof step?.uses !== "string" || !step.uses.includes("upload-artifact")) continue;
      const uploaded = typeof step?.with?.path === "string" ? step.with.path : "";
      if (EXPORTED_CREDENTIAL_PATH.test(uploaded)) {
        fail(
          "secret_boundary_violation",
          RELEASE_WORKFLOW,
          `job "${name}" uploads runner credential material as a workflow artifact`,
        );
      }
    }
  }

  const cleanup = stepsOf(buildJob).filter(
    (step) => typeof step?.run === "string" && /delete-keychain/.test(step.run),
  );
  if (cleanup.length === 0) {
    fail(
      "secret_boundary_violation",
      RELEASE_WORKFLOW,
      `job "${BUILD_JOB}" never removes the ephemeral signing credentials`,
    );
    return;
  }
  const unconditional = cleanup.filter((step) => String(step.if ?? "").includes("always()"));
  if (unconditional.length === 0) {
    fail(
      "secret_boundary_violation",
      RELEASE_WORKFLOW,
      `job "${BUILD_JOB}" removes the ephemeral signing credentials only on success`,
    );
    return;
  }

  // A value written to GITHUB_ENV only exists if the step that writes it ran to
  // completion. Cleanup that locates the credentials through such a value is
  // silently skipped exactly when it matters most: when import or identity
  // validation failed and the keychain is still on the runner.
  const exported = new Set(
    [...buildText.matchAll(/echo\s+"([A-Z_][A-Z0-9_]*)=/g)].map((match) => match[1]),
  );
  for (const step of unconditional) {
    const late = [...exported].filter((name) =>
      new RegExp(`\\$\\{?${name}\\b`).test(step.run),
    );
    if (late.length > 0) {
      fail(
        "secret_boundary_violation",
        RELEASE_WORKFLOW,
        `job "${BUILD_JOB}" cleanup depends on ${late.join(", ")}, which an earlier step may fail before exporting`,
      );
    }
  }
}

function checkUpdaterAbsence(workflowText, fail) {
  const found = UPDATER_ARTIFACTS.filter((rule) => rule.pattern.test(workflowText));
  if (found.length > 0) {
    fail(
      "updater_asset_present",
      RELEASE_WORKFLOW,
      `release workflow reintroduces an application updater surface: ${found.map((rule) => rule.id).join(", ")}`,
    );
  }
}

function checkVerificationGates(buildText, fail) {
  const missing = VERIFICATION_GATES.filter((gate) => !gate.pattern.test(buildText));
  if (missing.length > 0) {
    fail(
      "verification_gate_missing",
      RELEASE_WORKFLOW,
      `build job does not verify Apple trust: ${missing.map((gate) => gate.id).join(", ")}`,
    );
  }
}

function checkChecksums(jobs, fail) {
  const missing = CHECKSUM_GATES.filter((gate) => !gate.pattern.test(jobs.get(gate.job) ?? ""));
  if (missing.length > 0) {
    fail(
      "checksum_missing",
      RELEASE_WORKFLOW,
      `release workflow does not produce or re-verify SHA-256 checksums: ${missing
        .map((gate) => `${gate.job}/${gate.id}`)
        .join(", ")}`,
    );
  }
}

function checkPublicationOrder(workflowText, workflow, jobs, buildJob, fail) {
  const publishJob = workflow?.jobs?.[PUBLISH_JOB] ?? null;
  const publishText = jobs.get(PUBLISH_JOB) ?? "";

  if (!publishJob) {
    fail("publish_order_invalid", RELEASE_WORKFLOW, `job "${PUBLISH_JOB}" is missing`);
    return;
  }

  const needs = Array.isArray(publishJob.needs)
    ? publishJob.needs
    : typeof publishJob.needs === "string"
      ? [publishJob.needs]
      : [];
  const requiredNeeds = Object.keys(workflow?.jobs ?? {}).filter((name) => name !== PUBLISH_JOB);
  const missingNeeds = requiredNeeds.filter((name) => !needs.includes(name));
  if (missingNeeds.length > 0) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `job "${PUBLISH_JOB}" publishes without waiting for: ${missingNeeds.join(", ")}`,
    );
  }

  const condition = String(publishJob.if ?? "");
  if (!condition.includes("refs/tags/") || !condition.includes("github.event_name == 'push'")) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `job "${PUBLISH_JOB}" must run only for a pushed tag, so a manual run cannot publish`,
    );
  }

  const stagingGates = [
    { id: "existing-release-refused", pattern: /gh release view/ },
    { id: "draft-created", pattern: /gh release create[^\n]*--draft(?![=\w-])/ },
    { id: "draft-verified", pattern: /isDraft/ },
    { id: "complete-asset-set", pattern: /-(eq|ne) 4\b/ },
    { id: "published-after-verification", pattern: /--draft=false/ },
  ];
  const missingStaging = stagingGates.filter((gate) => !gate.pattern.test(publishText));
  if (missingStaging.length > 0) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `job "${PUBLISH_JOB}" does not stage and verify the draft before publishing: ${missingStaging
        .map((gate) => gate.id)
        .join(", ")}`,
    );
  }

  for (const { dmgArch } of EXPECTED_TARGETS) {
    if (!publishText.includes(`AgentDeck_\${VERSION}_${dmgArch}.dmg`)) {
      fail(
        "publish_order_invalid",
        RELEASE_WORKFLOW,
        `job "${PUBLISH_JOB}" does not require the ${dmgArch} disk image`,
      );
    }
  }

  if (/--clobber/.test(workflowText)) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      "release workflow overwrites existing release assets instead of refusing a reused tag",
    );
  }

  const targets = matrixTargets(buildJob);
  const missingTargets = EXPECTED_TARGETS.filter(({ target }) => !targets.includes(target));
  if (missingTargets.length > 0) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `build matrix does not cover ${missingTargets.map(({ arch }) => arch).join(", ")}`,
    );
  }

  const regressionText = jobs.get(REGRESSION_JOB) ?? null;
  if (regressionText === null) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `job "${REGRESSION_JOB}" is missing, so publication does not re-run the release-time regressions`,
    );
    return;
  }
  const missingRegressions = REGRESSION_GATES.filter((gate) => !gate.pattern.test(regressionText));
  if (missingRegressions.length > 0) {
    fail(
      "publish_order_invalid",
      RELEASE_WORKFLOW,
      `job "${REGRESSION_JOB}" does not re-run: ${missingRegressions.map((gate) => gate.id).join(", ")}`,
    );
  }
}

function checkDocumentation(rootDir, fail) {
  const doc = readText(rootDir, DISTRIBUTION_DOC);
  if (doc === null) {
    fail("documentation_incomplete", DISTRIBUTION_DOC, "official distribution guide is missing");
  } else {
    const missing = REQUIRED_DOC_TOPICS.filter((topic) => !topic.pattern.test(doc));
    if (missing.length > 0) {
      fail(
        "documentation_incomplete",
        DISTRIBUTION_DOC,
        `missing required topic(s): ${missing.map((topic) => topic.id).join(", ")}`,
      );
    }
    const forbidden = FORBIDDEN_DOC_CONTENT.filter((rule) => rule.pattern.test(doc));
    if (forbidden.length > 0) {
      fail(
        "documentation_incomplete",
        DISTRIBUTION_DOC,
        `contains forbidden content: ${forbidden.map((rule) => rule.id).join(", ")}`,
      );
    }
    if (!DISTRIBUTION_INACTIVE.test(leadingBlock(doc))) {
      fail(
        "documentation_incomplete",
        DISTRIBUTION_DOC,
        "missing required statement [distribution-inactive]: the guide must open by stating that no public AgentDeck release exists",
      );
    }
    const docClaims = unconditionalPublicationClaims(doc);
    if (docClaims.length > 0) {
      fail(
        "documentation_incomplete",
        DISTRIBUTION_DOC,
        `[active-release-claim] states a current public release as fact: "${docClaims[0]}"`,
      );
    }
    const credentials = CREDENTIAL_MATERIAL.filter((rule) => rule.pattern.test(doc));
    if (credentials.length > 0) {
      fail(
        "secret_boundary_violation",
        DISTRIBUTION_DOC,
        `contains credential material: ${credentials.map((rule) => rule.id).join(", ")}`,
      );
    }
  }

  const readme = readText(rootDir, READme);
  if (readme === null) {
    fail("documentation_incomplete", READme, "file is missing");
    return;
  }
  const missingReadme = REQUIRED_README_TOPICS.filter((topic) => !topic.pattern.test(readme));
  if (missingReadme.length > 0) {
    fail(
      "documentation_incomplete",
      READme,
      `missing required topic(s): ${missingReadme.map((topic) => topic.id).join(", ")}`,
    );
  }
  const readmeClaims = unconditionalPublicationClaims(readme);
  if (readmeClaims.length > 0) {
    fail(
      "documentation_incomplete",
      READme,
      `[active-release-claim] states a current public release as fact: "${readmeClaims[0]}"`,
    );
  }
  const credentials = CREDENTIAL_MATERIAL.filter((rule) => rule.pattern.test(readme));
  if (credentials.length > 0) {
    fail(
      "secret_boundary_violation",
      READme,
      `contains credential material: ${credentials.map((rule) => rule.id).join(", ")}`,
    );
  }
}

export function checkMacosDistribution({ rootDir }) {
  const failures = [];
  const fail = (code, location, message) => failures.push({ code, location, message });

  const packageJson = readJson(rootDir, "package.json");
  const tauriConfig = readJson(rootDir, "src-tauri/tauri.conf.json");
  const workflowText = readText(rootDir, RELEASE_WORKFLOW);

  if (workflowText === null) {
    fail("verification_gate_missing", RELEASE_WORKFLOW, "release workflow is missing");
    checkDocumentation(rootDir, fail);
    return { failures, summary: null };
  }

  const workflow = parseWorkflow(workflowText);
  const jobs = sliceJobs(workflowText);
  const buildJob = workflow?.jobs?.[BUILD_JOB] ?? null;
  const buildText = jobs.get(BUILD_JOB) ?? "";

  checkIdentity(workflowText, tauriConfig, fail);
  checkVersionBinding(packageJson, tauriConfig, buildText, fail);
  checkAuthority(workflow, jobs, fail);
  checkEnvironment(buildJob, fail);
  checkSecretBoundary(workflowText, workflow, jobs, buildJob, buildText, fail);
  checkUpdaterAbsence(workflowText, fail);
  checkVerificationGates(buildText, fail);
  checkChecksums(jobs, fail);
  checkPublicationOrder(workflowText, workflow, jobs, buildJob, fail);
  checkDocumentation(rootDir, fail);

  const summary =
    failures.length === 0
      ? `macOS distribution contract passed: product=${EXPECTED_PRODUCT_NAME} targets=${EXPECTED_TARGETS.map(
          ({ arch }) => arch,
        ).join(",")} updater=absent publish=staged`
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

  const { failures, summary } = checkMacosDistribution({ rootDir });
  if (summary) {
    console.log(summary);
    return;
  }

  console.error("macOS distribution contract failed:");
  for (const failure of failures) {
    console.error(`- [${failure.code}] ${failure.location}: ${failure.message}`);
  }
  process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli();
}
