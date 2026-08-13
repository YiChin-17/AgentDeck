#!/usr/bin/env node
// Static contract for the Plugins page.
//
// The page has no frontend test framework, so the wiring that keeps this route
// read-only is asserted against the source itself: a regression here would
// otherwise only show up as an installed, removed or disabled Plugin in the
// user's real CLI. Uses only the Node standard library.
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
const view = read('src', 'views', 'Plugins.tsx');
const pkg = JSON.parse(read('package.json') || '{}');
const en = JSON.parse(read('src', 'i18n', 'en.json') || '{}');
const zhTW = JSON.parse(read('src', 'i18n', 'zh-TW.json') || '{}');

// ── Route and navigation ──
require(
  app.includes('path="/plugins"') && app.includes('element={<Plugins />}'),
  'App.tsx must route /plugins to the Plugins view'
);
require(
  sidebar.includes('path: "/plugins"') && sidebar.includes('t("sidebar.plugins")'),
  'Sidebar must link to /plugins with a localized label'
);
require(
  pkg.scripts?.['check:plugins-ui'] === 'node scripts/check-plugins-ui.mjs',
  'package.json must expose this contract as check:plugins-ui'
);

// ── IPC contract: one request, and it carries nothing ──
require(
  api.includes('invoke<PluginInventory>("get_plugin_inventory")'),
  'tauri.ts must call get_plugin_inventory with no request fields'
);
require(
  api.includes('export const getPluginInventory = () =>'),
  'the getPluginInventory wrapper must take no parameters'
);
require(
  view.includes('api.getPluginInventory()'),
  'the Plugins view must load through the getPluginInventory wrapper'
);
const pluginApi = api.slice(api.indexOf('export const getPluginInventory'));
for (const forbidden of ['Path', 'executable', 'args', 'cwd', 'env']) {
  require(
    !pluginApi.includes(forbidden),
    `no Plugin command may take ${forbidden} from the frontend`
  );
}

// ── No mutation, validation, details or eval surface ──
const FORBIDDEN_COMMANDS = [
  'install_plugin',
  'update_plugin',
  'remove_plugin',
  'uninstall_plugin',
  'enable_plugin',
  'disable_plugin',
  'validate_plugin',
  'plugin_details',
  'plugin_eval',
  'add_plugin_marketplace',
  'remove_plugin_marketplace',
  'update_plugin_marketplace',
];
for (const command of FORBIDDEN_COMMANDS) {
  require(!api.includes(command), `tauri.ts must not expose ${command}`);
  require(!view.includes(command), `the Plugins view must not call ${command}`);
}
const FORBIDDEN_CALLS = ['invoke(', 'Command(', 'shell', 'writeTextFile', 'api.installLocal'];
for (const call of FORBIDDEN_CALLS) {
  require(!view.includes(call), `the Plugins view must not call ${call}`);
}
for (const label of [
  'installPlugin',
  'updatePlugin',
  'removePlugin',
  'uninstallPlugin',
  'enablePlugin',
  'disablePlugin',
  'validatePlugin',
  'pluginDetails',
  'runEval',
]) {
  require(!view.includes(label), `the Plugins page must not offer ${label}`);
}
require(
  view.includes('t("plugins.readOnlyBadge")'),
  'the Plugins page must label itself read-only'
);

// ── The five filters ──
for (const key of ['agent', 'presence', 'scope', 'marketplace', 'status']) {
  require(
    view.includes(`t("plugins.filter.${key}")`),
    `the Plugins page must expose the ${key} filter`
  );
}
require(
  view.includes('t("plugins.filter.all")'),
  'every filter must offer an unfiltered option'
);
require(
  view.includes('"installed", "available"') &&
    view.includes('presenceFilter === "available"') &&
    view.includes('item.available === "available"'),
  'the presence filter must distinguish installed and available records'
);

// ── Unknown stays visible as unknown ──
require(
  view.includes('t("plugins.unknown")'),
  'unknown values must render as unknown, not as a blank or a false'
);
for (const state of ['installed', 'available', 'enabled', 'update']) {
  require(
    view.includes(`plugins.${state}.`),
    `the ${state} state of an item must be rendered from its own localized key`
  );
}

// ── Diagnostics survive the item filters ──
require(
  view.includes('agent.diagnostics.map') && view.includes('t(`plugins.diagnostic.'),
  'per-Agent diagnostics must render from the unfiltered response'
);
require(
  !/visibleItems[\s\S]{0,400}diagnostics/.test(view),
  'diagnostics must not be derived from the filtered item list'
);
require(
  view.includes('t("plugins.cliMissingHint")'),
  'an Agent whose CLI is absent must render as a normal empty state'
);

// ── Route-local state only ──
require(
  !view.includes('localStorage.'),
  'no Plugin inventory may reach localStorage'
);
require(
  !view.includes('useApp()') && !view.includes('AppContext'),
  'no Plugin inventory may reach the shared app context'
);

// ── Latest request wins ──
require(
  view.includes('requestIdRef') && view.includes('if (requestIdRef.current !== requestId) return;'),
  'the Plugins view must drop responses from superseded requests'
);

// ── Bilingual coverage ──
const REQUIRED_KEYS = [
  'sidebar.plugins',
  'plugins.title',
  'plugins.subtitle',
  'plugins.readOnlyBadge',
  'plugins.loadFailed',
  'plugins.unknown',
  'plugins.cliMissingHint',
  'plugins.cliVersion',
  'plugins.capabilities',
  'plugins.marketplaces',
  'plugins.noItems',
  'plugins.itemsHeading',
  'plugins.detailsHeading',
  'plugins.filter.all',
  'plugins.filter.agent',
  'plugins.filter.presence',
  'plugins.filter.scope',
  'plugins.filter.marketplace',
  'plugins.filter.status',
  'plugins.agent.codex',
  'plugins.agent.claude_code',
  'plugins.installed.installed',
  'plugins.installed.not_installed',
  'plugins.installed.unknown',
  'plugins.available.available',
  'plugins.available.unknown',
  'plugins.scope.user',
  'plugins.scope.project',
  'plugins.scope.local',
  'plugins.scope.unknown',
  'plugins.enabled.enabled',
  'plugins.enabled.disabled',
  'plugins.enabled.unknown',
  'plugins.update.unknown',
  'plugins.capability.version',
  'plugins.capability.plugin_list',
  'plugins.capability.marketplace_list',
  'plugins.diagnostic.cli_missing',
  'plugins.diagnostic.unsupported_cli',
  'plugins.diagnostic.timeout',
  'plugins.diagnostic.non_zero_exit',
  'plugins.diagnostic.invalid_json',
  'plugins.diagnostic.output_too_large',
  'plugins.diagnostic.marketplace_unavailable',
  'plugins.installedVersion',
  'plugins.availableVersion',
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
  console.error(`Plugins UI check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Plugins UI check passed.');
