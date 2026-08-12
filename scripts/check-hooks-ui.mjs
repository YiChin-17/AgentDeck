#!/usr/bin/env node
// Static contract for the read-only Hooks page.
//
// The page has no frontend test framework, so the wiring that keeps it
// read-only and keeps the diff bounded is asserted against the source itself:
// a regression here would otherwise only show up as a running Hook or a frozen
// UI. Uses only the Node standard library.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const read = (...parts) => fs.readFileSync(path.join(root, ...parts), 'utf8');

const app = read('src', 'App.tsx');
const sidebar = read('src', 'components', 'Sidebar.tsx');
const api = read('src', 'lib', 'tauri.ts');
const view = read('src', 'views', 'Hooks.tsx');
const inspector = read('src', 'components', 'HookInspector.tsx');
const en = JSON.parse(read('src', 'i18n', 'en.json'));
const zhTW = JSON.parse(read('src', 'i18n', 'zh-TW.json'));

const errors = [];
const require = (condition, message) => {
  if (!condition) errors.push(message);
};

// ── Route and navigation ──
require(
  app.includes('path="/hooks"') && app.includes('element={<Hooks />}'),
  'App.tsx must route /hooks to the Hooks view'
);
require(
  sidebar.includes('path: "/hooks"') && sidebar.includes('t("sidebar.hooks")'),
  'Sidebar must link to /hooks with a localized label'
);

// ── IPC contract ──
require(
  api.includes('invoke<HookInspection>("get_hook_inspection", { projectId })'),
  'tauri.ts must call get_hook_inspection with a projectId argument only'
);
require(
  view.includes('api.getHookInspection('),
  'the Hooks view must load through the getHookInspection wrapper'
);

// ── Read-only surface ──
const MUTATION_CALLS = [
  'api.installLocal',
  'api.deleteManagedSkill',
  'api.exportSkillToProject',
  'api.toggleProjectSkill',
  'api.gitBackup',
  'writeTextFile',
  'invoke(',
];
for (const call of MUTATION_CALLS) {
  require(!view.includes(call), `the Hooks view must not call ${call}`);
}
require(
  (view.match(/api\.[a-zA-Z]+\(/g) ?? []).every((call) => call === 'api.getHookInspection('),
  'the Hooks view must call no backend command other than getHookInspection'
);
require(
  view.includes('t("hooks.readOnlyBadge")'),
  'the Hooks page must label its capability as read-only'
);

// ── Filters ──
for (const key of ['agent', 'scope', 'event', 'status']) {
  require(
    view.includes(`t("hooks.filter.${key}")`),
    `the Hooks page must expose the ${key} filter`
  );
}
require(
  view.includes('t("hooks.projectLabel")') && view.includes('t("hooks.projectNone")'),
  'the Hooks page must expose a Project selector with a user-scope-only option'
);

// ── Latest request wins ──
require(
  view.includes('requestIdRef') &&
    view.includes('if (requestIdRef.current !== requestId) return;'),
  'the Hooks view must drop responses from superseded requests'
);

// ── Source diagnostics and states ──
for (const status of ['valid', 'missing', 'invalid', 'too_large']) {
  require(
    view.includes(`hooks.status.${status}`) || view.includes(`"${status}"`),
    `the Hooks page must render the ${status} source state`
  );
}
require(
  view.includes('source.diagnostic') && view.includes('hooks.diagnostic.'),
  'the Hooks page must render the per-source diagnostic'
);
require(
  view.includes('t("hooks.missingHint")'),
  'a missing source must render as a normal empty state'
);

// ── Inspector ──
require(
  view.includes('<HookInspector') && view.includes('entry={selectedEntry}'),
  'selecting an entry must open the Hook Inspector'
);
for (const key of ['event', 'matcher', 'handlerType', 'source', 'fields']) {
  require(
    inspector.includes(`t("hooks.inspector.${key}")`),
    `the Inspector must show the ${key} of an entry`
  );
}
require(
  inspector.includes('!entry.eventKnown') &&
    inspector.includes('!entry.handlerTypeKnown') &&
    inspector.includes('!field.known'),
  'the Inspector must mark unknown events, handler types and fields'
);

// ── Bounded, same-Agent comparison ──
require(
  view.includes('left.agent !== right.agent') && view.includes('"cross_agent"'),
  'comparison must refuse a cross-Agent pair'
);
require(
  view.includes('left.id === right.id') && view.includes('"same_source"'),
  'comparison must refuse the same source twice'
);
require(
  view.includes('!left.diffAvailable || !right.diffAvailable') && view.includes('"not_diffable"'),
  'comparison must refuse a source the backend marked non-diffable'
);
require(
  view.includes('refusal ?') && view.includes('<DocumentDiffViewer'),
  'the diff component must render only when no refusal reason applies'
);
require(
  view.includes('original={compareLeft.canonicalText}') &&
    view.includes('updated={compareRight.canonicalText}'),
  'the diff must run on canonical Hook text, not whole config documents'
);

// ── Compatibility matrix ──
require(
  view.includes('data.compatibility.map') &&
    view.includes('row.codex.support') &&
    view.includes('row.claudeCode.support'),
  'the page must render both Agent columns of the compatibility matrix'
);
require(
  view.includes('hooks.note.') && view.includes('t("hooks.matrixSnapshot"'),
  'the matrix must carry Agent-specific notes and its snapshot date'
);

// ── Bilingual coverage ──
const REQUIRED_KEYS = [
  'sidebar.hooks',
  'hooks.readOnlyBadge',
  'hooks.filter.agent',
  'hooks.filter.status',
  'hooks.status.too_large',
  'hooks.diagnostic.invalid_syntax',
  'hooks.compareRefusal.cross_agent',
  'hooks.support.unknown',
  'hooks.note.shared_name_distinct_contract',
  'hooks.inspector.handlerType',
];
for (const key of REQUIRED_KEYS) {
  for (const [name, locale] of [['en', en], ['zh-TW', zhTW]]) {
    const value = key.split('.').reduce((node, part) => (node ? node[part] : undefined), locale);
    require(typeof value === 'string' && value.length > 0, `${name}.json is missing ${key}`);
  }
}

if (errors.length) {
  console.error(`Hooks UI check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Hooks UI check passed.');
