#!/usr/bin/env node
// Static contract for the Config Profiles page.
//
// The page has no frontend test framework, so the wiring that keeps this route
// inspection-only is asserted against the source itself: a regression here would
// otherwise show up as a written, repaired or reformatted config file in the
// user's real Codex or Claude Code installation. Uses only the Node standard
// library.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const errors = [];
const require = (condition, message) => {
  if (!condition) errors.push(message);
};

// A missing file is a contract failure, not a crash: this check runs before the
// page exists and has to report what is missing.
const read = (...parts) => {
  const file = path.join(root, ...parts);
  try {
    return fs.readFileSync(file, 'utf8');
  } catch {
    errors.push(`${path.join(...parts)} does not exist`);
    return '';
  }
};

const app = read('src', 'App.tsx');
const sidebar = read('src', 'components', 'Sidebar.tsx');
const api = read('src', 'lib', 'tauri.ts');
const view = read('src', 'views', 'ConfigProfiles.tsx');
const pkg = JSON.parse(read('package.json') || '{}');
const en = JSON.parse(read('src', 'i18n', 'en.json') || '{}');
const zhTW = JSON.parse(read('src', 'i18n', 'zh-TW.json') || '{}');

// ── Route and navigation ──
require(
  app.includes('path="/config-profiles"') && app.includes('element={<ConfigProfiles />}'),
  'App.tsx must route /config-profiles to the ConfigProfiles view'
);
require(
  sidebar.includes('path: "/config-profiles"') && sidebar.includes('t("sidebar.configProfiles")'),
  'Sidebar must link to /config-profiles with a localized label'
);
require(
  pkg.scripts?.['check:config-profiles-ui'] === 'node scripts/check-config-profiles-ui.mjs',
  'package.json must expose this contract as check:config-profiles-ui'
);

// ── IPC contract: one read-only request, and it carries no location ──
require(
  api.includes('invoke<ConfigProfileInventory>("get_config_profile_inventory", { request })'),
  'tauri.ts must call get_config_profile_inventory with a single request object'
);
require(
  api.includes('export const getConfigProfileInventory = (request: ConfigInventoryRequest)'),
  'the getConfigProfileInventory wrapper must take exactly one typed request'
);
require(
  view.includes('api.getConfigProfileInventory('),
  'the ConfigProfiles view must load through the getConfigProfileInventory wrapper'
);

// The request type is the whole frontend authority over what gets read.
const requestType = api.slice(
  api.indexOf('export interface ConfigInventoryRequest'),
  api.indexOf('export interface ConfigSource')
);
require(requestType.length > 0, 'tauri.ts must declare ConfigInventoryRequest before ConfigSource');
require(
  /projectId\??:\s*string \| null;/.test(requestType) &&
    /agent\??:\s*ConfigAgentKey \| null;/.test(requestType) &&
    /scope\??:\s*ConfigScopeKey \| null;/.test(requestType),
  'ConfigInventoryRequest must carry exactly projectId, agent and scope'
);
for (const forbidden of ['path', 'Path', 'cwd', 'env', 'home', 'root', 'raw', 'file']) {
  require(
    !requestType.includes(forbidden),
    `ConfigInventoryRequest must not accept ${forbidden} from the frontend`
  );
}

// ── Inspection itself still has no side effect ──
//
// The page now also hosts a preview-first management workflow, so the blanket
// ban on mutation wrappers moved to `check-config-profile-management.mjs`.
// What stays here is the part that was never about management: the inventory
// request reads, and the view reaches IPC only through typed wrappers.
for (const call of ['invoke(', 'Command(', 'writeTextFile', 'removeFile', 'shell']) {
  require(!view.includes(call), `the ConfigProfiles view must not call ${call}`);
}
// The read path is one command. A second read wrapper would be a second
// authority over which files get opened.
require(
  (api.match(/get_config_profile_inventory/g) || []).length === 1,
  'the inventory must be reachable through exactly one command'
);

// ── The three filters, refresh and the runtime limitation ──
for (const key of ['agent', 'scope', 'project']) {
  require(
    view.includes(`t("configProfiles.filter.${key}")`),
    `the Config Profiles page must expose the ${key} filter`
  );
}
require(
  view.includes('t("configProfiles.filter.all")'),
  'every filter must offer an unfiltered option'
);
require(view.includes('t("common.refresh")'), 'the page must offer a refresh control');
require(
  view.includes('t("configProfiles.runtimeLimitation")'),
  'the page must state that CLI flags, environment and managed policy are outside the resolution'
);

// ── Sources, diagnostics, settings and diff all render ──
require(
  view.includes('t(`configProfiles.status.') && view.includes('inventory.sources.map'),
  'every fixed source must render its own status'
);
require(
  view.includes('t(`configProfiles.diagnostic.') && view.includes('inventory.diagnostics.map'),
  'typed diagnostics must render from the response'
);
require(
  view.includes('t(`configProfiles.resolution.') && view.includes('visibleSettings.map'),
  'each normalized setting must render its observed resolution'
);
require(
  view.includes('t(`configProfiles.diff.') && view.includes('inventory.diffs.map'),
  'the normalized diff must render its four statuses'
);
require(
  view.includes('t("configProfiles.hasUnexposedFields")'),
  'a source with unexposed content must say so without naming it'
);

// ── Empty states stay distinguishable ──
require(
  view.includes('t("configProfiles.noSettings")') &&
    view.includes('t("configProfiles.sourceFailureHint")') &&
    view.includes('t("configProfiles.noProjects")'),
  'an empty inventory, a failed source and an unregistered project must be different states'
);

// ── Route-local state only ──
require(!view.includes('localStorage.'), 'no config content may reach localStorage');

// ── Latest request wins ──
require(
  view.includes('requestIdRef') && view.includes('if (requestIdRef.current !== requestId) return;'),
  'the ConfigProfiles view must drop responses from superseded requests'
);

// ── Bilingual coverage ──
const REQUIRED_KEYS = [
  'sidebar.configProfiles',
  'configProfiles.title',
  'configProfiles.subtitle',
  'configProfiles.readOnlyBadge',
  'configProfiles.runtimeLimitation',
  'configProfiles.loadFailed',
  'configProfiles.projectNotFound',
  'configProfiles.noProjects',
  'configProfiles.noSettings',
  'configProfiles.noDiff',
  'configProfiles.sourceFailureHint',
  'configProfiles.hasUnexposedFields',
  'configProfiles.sourcesHeading',
  'configProfiles.settingsHeading',
  'configProfiles.diffHeading',
  'configProfiles.diagnosticsHeading',
  'configProfiles.filter.all',
  'configProfiles.filter.agent',
  'configProfiles.filter.scope',
  'configProfiles.filter.project',
  'configProfiles.projectNone',
  'configProfiles.agent.codex',
  'configProfiles.agent.claude_code',
  'configProfiles.scope.user',
  'configProfiles.scope.project',
  'configProfiles.scope.project_local',
  'configProfiles.status.missing',
  'configProfiles.status.available',
  'configProfiles.status.unreadable',
  'configProfiles.status.too_large',
  'configProfiles.status.unsupported_symlink',
  'configProfiles.status.invalid_format',
  'configProfiles.diagnostic.unreadable',
  'configProfiles.diagnostic.too_large',
  'configProfiles.diagnostic.unsupported_symlink',
  'configProfiles.diagnostic.invalid_format',
  'configProfiles.diagnostic.invalid_allowed_value',
  'configProfiles.resolution.observed_active',
  'configProfiles.resolution.observed_overridden',
  'configProfiles.resolution.project_candidate',
  'configProfiles.diff.same',
  'configProfiles.diff.added',
  'configProfiles.diff.changed',
  'configProfiles.diff.removed',
  'configProfiles.column.key',
  'configProfiles.column.value',
  'configProfiles.column.scope',
  'configProfiles.column.source',
  'configProfiles.column.resolution',
  'configProfiles.column.agent',
  'configProfiles.column.status',
  'configProfiles.column.path',
  'configProfiles.column.fingerprint',
];
for (const key of REQUIRED_KEYS) {
  for (const [name, locale] of [
    ['en', en],
    ['zh-TW', zhTW],
  ]) {
    const value = key.split('.').reduce((node, part) => (node ? node[part] : undefined), locale);
    require(typeof value === 'string' && value.length > 0, `${name}.json is missing ${key}`);
  }
}

if (errors.length) {
  console.error(`Config Profiles UI check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Config Profiles UI check passed.');
