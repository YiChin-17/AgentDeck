#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const EXPECTED_PRODUCT_NAME = 'AgentDeck';

/// Every file a prepared release rewrites. The list is exported and printed by
/// `--list-files` so the workflow stages exactly these paths: the previous
/// hard-coded `git add` still named `src/i18n/zh.json`, a locale file that had
/// already been renamed, so a rename silently dropped a file from the release
/// commit.
export const RELEASE_FILES = [
  'CHANGELOG.md',
  'CHANGELOG-zh-TW.md',
  'package.json',
  'src-tauri/tauri.conf.json',
  'src/i18n/en.json',
  'src/i18n/zh-TW.json',
];

const LOCALE_FILES = ['src/i18n/en.json', 'src/i18n/zh-TW.json'];

function parseSemver(version) {
  const m = String(version ?? '').match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!m) return null;
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]) };
}

export function bumpVersion(current, releaseType) {
  const parsed = parseSemver(current);
  if (!parsed) {
    throw new Error(`Current package version is not SemVer: ${current}`);
  }

  if (releaseType === 'patch') {
    return `${parsed.major}.${parsed.minor}.${parsed.patch + 1}`;
  }
  if (releaseType === 'minor') {
    return `${parsed.major}.${parsed.minor + 1}.0`;
  }
  if (releaseType === 'major') {
    return `${parsed.major + 1}.0.0`;
  }

  if (parseSemver(releaseType)) {
    return releaseType;
  }

  throw new Error(`Invalid release type/version: ${releaseType}`);
}

/// A release commit is the single traceable origin of a tagged AgentDeck
/// release, so anything that would make the tag ambiguous — a reused tag, a
/// commit outside protected main history, versions that already disagree, or a
/// bundle that is not AgentDeck — has to stop the bump before any file is
/// rewritten.
export function releaseBlockers({
  nextVersion,
  branch,
  tags,
  packageVersion,
  bundleVersion,
  productName,
}) {
  const blockers = [];

  if (!parseSemver(nextVersion)) {
    blockers.push({ code: 'invalid_version', message: `"${nextVersion}" is not a SemVer version` });
  }
  if (productName !== EXPECTED_PRODUCT_NAME) {
    blockers.push({
      code: 'identity_mismatch',
      message: `src-tauri/tauri.conf.json productName is "${productName}", expected "${EXPECTED_PRODUCT_NAME}"`,
    });
  }
  if (packageVersion !== bundleVersion) {
    blockers.push({
      code: 'version_mismatch',
      message: `package.json "${packageVersion}" and src-tauri/tauri.conf.json "${bundleVersion}" already disagree`,
    });
  }
  if ((tags ?? []).includes(`v${nextVersion}`)) {
    blockers.push({
      code: 'tag_reused',
      message: `tag v${nextVersion} already exists; a released version is never rebuilt`,
    });
  }
  if (branch !== 'main') {
    blockers.push({
      code: 'unprotected_branch',
      message: `release commits are prepared on main, current branch is "${branch ?? 'unknown'}"`,
    });
  }

  return blockers;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function updateSettingsVersion(i18nObj, nextVersion, fileLabel) {
  if (!i18nObj.settings || typeof i18nObj.settings.version !== 'string') {
    throw new Error(`Missing settings.version in ${fileLabel}`);
  }
  i18nObj.settings.version = i18nObj.settings.version.replace(/\d+\.\d+\.\d+/, nextVersion);
}

function ensureChangelogEntry(changelog, nextVersion, dateStr, { zh = false } = {}) {
  const heading = `## [${nextVersion}] - ${dateStr}`;
  if (changelog.includes(heading) || changelog.includes(`## [${nextVersion}] -`)) {
    return changelog;
  }

  const sections = zh
    ? ['### 發布概覽', '- ', '', '### 使用者可見的更新', '- ', '', '### 開發與治理', '- ']
    : ['### Release Overview', '- ', '', '### User-facing', '- ', '', '### Developer & Governance', '- '];

  const entry = [heading, '', ...sections, ''].join('\n');

  const firstReleaseHeading = changelog.search(/^## \[/m);
  if (firstReleaseHeading === -1) {
    return `${changelog.trimEnd()}\n\n${entry}\n`;
  }

  return `${changelog.slice(0, firstReleaseHeading)}${entry}${changelog.slice(firstReleaseHeading)}`;
}

function readGitState(root) {
  const run = (args) => {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    return result.status === 0 ? result.stdout.trim() : null;
  };
  const branch = run(['rev-parse', '--abbrev-ref', 'HEAD']);
  const tagOutput = run(['tag', '--list']);
  return { branch, tags: tagOutput ? tagOutput.split('\n').filter(Boolean) : [] };
}

function main() {
  const root = process.cwd();
  const args = process.argv.slice(2);

  if (args.includes('--list-files')) {
    console.log(RELEASE_FILES.join('\n'));
    return;
  }

  const releaseArg = args.find((arg) => !arg.startsWith('--'));
  const dryRun = args.includes('--dry-run');

  if (!releaseArg) {
    console.error('Usage: npm run release:prepare -- <patch|minor|major|x.y.z> [--dry-run]');
    process.exitCode = 1;
    return;
  }

  const missing = RELEASE_FILES.filter((file) => !fs.existsSync(path.join(root, file)));
  if (missing.length > 0) {
    console.error(`Cannot prepare a release, these files are missing: ${missing.join(', ')}`);
    process.exitCode = 1;
    return;
  }

  const pkg = readJson(path.join(root, 'package.json'));
  const tauriConf = readJson(path.join(root, 'src-tauri', 'tauri.conf.json'));
  const currentVersion = pkg.version;

  let nextVersion;
  try {
    nextVersion = bumpVersion(currentVersion, releaseArg);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
    return;
  }

  const { branch, tags } = readGitState(root);
  const blockers = releaseBlockers({
    nextVersion,
    branch,
    tags,
    packageVersion: currentVersion,
    bundleVersion: tauriConf.version,
    productName: tauriConf.productName,
  });
  if (blockers.length > 0) {
    console.error(`Refusing to prepare release ${nextVersion}:`);
    for (const blocker of blockers) {
      console.error(`- [${blocker.code}] ${blocker.message}`);
    }
    process.exitCode = 1;
    return;
  }

  const dateStr = new Date().toISOString().slice(0, 10);
  const locales = LOCALE_FILES.map((relativePath) => {
    const value = readJson(path.join(root, relativePath));
    updateSettingsVersion(value, nextVersion, relativePath);
    return [relativePath, value];
  });

  const changelogPath = path.join(root, 'CHANGELOG.md');
  const changelogZhPath = path.join(root, 'CHANGELOG-zh-TW.md');
  const nextChangelog = ensureChangelogEntry(
    fs.readFileSync(changelogPath, 'utf8'),
    nextVersion,
    dateStr,
  );
  const nextChangelogZh = ensureChangelogEntry(
    fs.readFileSync(changelogZhPath, 'utf8'),
    nextVersion,
    dateStr,
    { zh: true },
  );

  if (dryRun) {
    console.log(`[dry-run] ${currentVersion} -> ${nextVersion}`);
    return;
  }

  pkg.version = nextVersion;
  tauriConf.version = nextVersion;
  writeJson(path.join(root, 'package.json'), pkg);
  writeJson(path.join(root, 'src-tauri', 'tauri.conf.json'), tauriConf);
  for (const [relativePath, value] of locales) {
    writeJson(path.join(root, relativePath), value);
  }
  fs.writeFileSync(changelogPath, nextChangelog);
  fs.writeFileSync(changelogZhPath, nextChangelogZh);

  console.log(`Prepared release ${nextVersion}`);
  console.log('Updated:');
  for (const relativePath of RELEASE_FILES) {
    console.log(`- ${relativePath}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
