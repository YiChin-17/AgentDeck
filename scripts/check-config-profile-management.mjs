#!/usr/bin/env node
// Static contract for Config Profile management.
//
// The page has no frontend test framework, so the wiring that keeps mutation
// preview-first and scope-bounded is asserted against the source itself: a
// regression here would otherwise show up as an unreviewed write into the
// user's real Codex or Claude Code project configuration. Uses only the Node
// standard library.
//
// The companion check `check-config-profiles-ui.mjs` still owns the inspection
// half of the page. This file owns the management half and the boundary
// between them.
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

const api = read('src', 'lib', 'tauri.ts');
const view = read('src', 'views', 'ConfigProfiles.tsx');
const pkg = JSON.parse(read('package.json') || '{}');
const en = JSON.parse(read('src', 'i18n', 'en.json') || '{}');
const zhTW = JSON.parse(read('src', 'i18n', 'zh-TW.json') || '{}');

require(
  pkg.scripts?.['check:config-profile-management'] ===
    'node scripts/check-config-profile-management.mjs',
  'package.json must expose this contract as check:config-profile-management'
);

// ── The inspection contract does not regress ──
//
// Management lives beside inspection, not instead of it. These are the same
// controls the inspection-only requirement guaranteed, re-asserted here so a
// management rewrite cannot quietly drop them.
for (const key of ['agent', 'scope', 'project']) {
  require(
    view.includes(`t("configProfiles.filter.${key}")`),
    `the ${key} inventory filter must survive alongside management`
  );
}
require(
  view.includes('api.getConfigProfileInventory('),
  'the inventory must still load through the read-only wrapper'
);
require(
  view.includes('t("configProfiles.runtimeLimitation")'),
  'the runtime limitation notice must survive alongside management'
);
require(
  view.includes('inventory.diagnostics.map') && view.includes('inventory.diffs.map'),
  'diagnostics and the normalized diff must survive alongside management'
);

// ── Every management command goes through a typed wrapper ──
const WRAPPERS = [
  'listConfigProfiles',
  'listConfigProfileKeys',
  'createConfigProfile',
  'updateConfigProfile',
  'deleteConfigProfile',
  'listConfigProfileAssignments',
  'setConfigProfileAssignment',
  'removeConfigProfileAssignment',
  'previewConfigProfileApply',
  'applyConfigProfile',
  'previewConfigProfileRestore',
  'applyConfigProfileRestore',
];
for (const wrapper of WRAPPERS) {
  require(api.includes(`export const ${wrapper} = `), `tauri.ts must export ${wrapper}`);
}
for (const command of [
  'list_config_profiles',
  'list_config_profile_keys',
  'create_config_profile',
  'update_config_profile',
  'delete_config_profile',
  'list_config_profile_assignments',
  'set_config_profile_assignment',
  'remove_config_profile_assignment',
  'preview_config_profile_apply',
  'apply_config_profile',
  'preview_config_profile_restore',
  'apply_config_profile_restore',
]) {
  require(api.includes(`"${command}"`), `tauri.ts must invoke ${command}`);
}
// The view never reaches the IPC layer directly: every call is a typed wrapper.
for (const call of ['invoke(', 'Command(', 'writeTextFile', 'removeFile', 'shell']) {
  require(!view.includes(call), `the ConfigProfiles view must not call ${call}`);
}

// ── Requests carry ids and typed scalars, never a location ──
const sliceBetween = (text, start, end) => {
  const from = text.indexOf(start);
  if (from < 0) return '';
  const to = text.indexOf(end, from + start.length);
  return to < 0 ? text.slice(from) : text.slice(from, to);
};

const REQUEST_TYPES = [
  'ConfigProfileEntryInput',
  'CreateConfigProfileRequest',
  'UpdateConfigProfileRequest',
  'ConfigProfileAssignmentRequest',
  'ConfigProfilePreviewRequest',
  'ConfigProfileApplyRequest',
];
// Checked against the declared field names rather than the whole text: a
// substring match would flag `profileId` for containing "file".
const fieldNames = (body) =>
  body
    .split('\n')
    .map((line) => line.match(/^\s*([A-Za-z0-9_]+)\??\s*:/))
    .filter(Boolean)
    .map((match) => match[1]);

for (const name of REQUEST_TYPES) {
  const declared = `export interface ${name}`;
  require(api.includes(declared), `tauri.ts must declare ${name}`);
  const fields = fieldNames(sliceBetween(api, declared, '\n}'));
  for (const forbidden of [
    'path',
    'sourcePath',
    'targetPath',
    'cwd',
    'env',
    'home',
    'root',
    'raw',
    'rawDocument',
    'documentText',
    'file',
    'fileName',
    'scope',
    'command',
  ]) {
    require(
      !fields.some((field) => field.toLowerCase() === forbidden.toLowerCase()),
      `${name} must not accept ${forbidden} from the frontend`
    );
  }
}
// Apply and restore confirm a token and nothing else.
const applyRequest = sliceBetween(api, 'export interface ConfigProfileApplyRequest', '\n}');
require(
  /token:\s*string;/.test(applyRequest) &&
    applyRequest.split('\n').filter((line) => line.includes(':')).length === 1,
  'ConfigProfileApplyRequest must carry exactly one field: token'
);

// ── The editor offers only allowlisted typed keys ──
require(
  view.includes('api.listConfigProfileKeys('),
  'the typed editor must build its controls from the backend allowlist'
);
require(
  view.includes('valueKind') && view.includes('canonicalKey'),
  'each editor control must be driven by the key vocabulary and its scalar kind'
);

// ── Assignment names a registered Project and an Agent, nothing else ──
require(
  view.includes('t("configProfiles.manage.assignHeading")') &&
    view.includes('t("configProfiles.manage.assign")') &&
    view.includes('t("configProfiles.manage.unassign")'),
  'the page must offer assignment and removal for a registered Project and Agent'
);

// ── Apply and restore are preview-first, and confirm sends only the token ──
require(
  view.includes('api.previewConfigProfileApply(') &&
    view.includes('api.previewConfigProfileRestore('),
  'apply and restore must both go through a preview'
);
require(
  view.includes('api.applyConfigProfile({ token:') &&
    view.includes('api.applyConfigProfileRestore({ token:'),
  'confirm must submit only the preview token'
);
require(
  view.includes('t("configProfiles.manage.previewHeading")') &&
    view.includes('t("configProfiles.manage.confirm")') &&
    view.includes('t("common.cancel")'),
  'the preview dialog must name the operation and offer confirm and cancel'
);
// The dialog identifies what is about to change.
for (const field of ['preview.profileName', 'preview.projectId', 'preview.sourceId']) {
  require(
    view.includes(field),
    `the preview dialog must identify ${field.split('.')[1]}`
  );
}
require(
  view.includes('preview.diff.map'),
  'the preview dialog must render the typed diff it was given'
);

// ── Cancel is a pure state reset ──
//
// Asserted on the handler itself: a cancel that called an apply would be the
// one bug this whole preview flow exists to prevent.
const cancelHandler = sliceBetween(view, 'const cancelPreview =', '};');
require(cancelHandler.length > 0, 'the view must define a cancelPreview handler');
for (const call of ['api.applyConfigProfile', 'api.applyConfigProfileRestore', 'await api.']) {
  require(
    !cancelHandler.includes(call),
    `cancelPreview must not call ${call}`
  );
}

// ── A stale preview keeps the selection and requires a fresh review ──
require(
  view.includes('stale_preview') && view.includes('preview_expired'),
  'the page must recognize the stale and expired preview codes'
);
require(
  view.includes('setPreview(null)'),
  'a stale preview must be discarded so confirm cannot be pressed again'
);

// ── Double confirm is blocked while a mutation is in flight ──
require(
  view.includes('busy') && /disabled=\{[^}]*busy/.test(view),
  'confirm must be disabled while a mutation is in flight'
);

// ── Nothing outside the approved scope is expressible ──
//
// Scoped to the management component: the inspection half legitimately names
// the user and project-local scopes, because it reads them.
const managementSource = view.slice(view.indexOf('function ConfigProfileManagement'));
require(
  managementSource.length > 0,
  'the view must define the management component as ConfigProfileManagement'
);
for (const forbidden of [
  'settings.local.json',
  'project_local',
  'userScope',
  'batchApply',
  'applyAll',
  'autoApply',
  'schedule',
  'watcher',
  'rawEditor',
  'documentText',
  'apiKey',
  'secret',
  'credential',
]) {
  require(
    !managementSource.includes(forbidden),
    `the management UI must not offer ${forbidden}`
  );
}
require(!view.includes('localStorage.'), 'no config content may reach localStorage');

// ── Latest request wins, and a success refreshes everything visible ──
require(
  view.includes('requestIdRef') && view.includes('if (requestIdRef.current !== requestId) return;'),
  'the view must drop responses from superseded requests'
);
const refreshAll = sliceBetween(view, 'const refreshManagement =', '};');
require(refreshAll.length > 0, 'the view must define a refreshManagement handler');
for (const call of [
  'listConfigProfiles',
  'listConfigProfileAssignments',
]) {
  require(
    refreshAll.includes(call),
    `refreshManagement must reload ${call}`
  );
}

// ── Bilingual coverage ──
const REQUIRED_KEYS = [
  'configProfiles.manage.heading',
  'configProfiles.manage.subtitle',
  'configProfiles.manage.newProfile',
  'configProfiles.manage.profileName',
  'configProfiles.manage.save',
  'configProfiles.manage.delete',
  'configProfiles.manage.deleteConfirm',
  'configProfiles.manage.revision',
  'configProfiles.manage.noProfiles',
  'configProfiles.manage.entriesHeading',
  'configProfiles.manage.unset',
  'configProfiles.manage.assignHeading',
  'configProfiles.manage.assign',
  'configProfiles.manage.unassign',
  'configProfiles.manage.noAssignments',
  'configProfiles.manage.applyAction',
  'configProfiles.manage.restoreAction',
  'configProfiles.manage.previewHeading',
  'configProfiles.manage.previewApply',
  'configProfiles.manage.previewRestore',
  'configProfiles.manage.confirm',
  'configProfiles.manage.wouldCreateFile',
  'configProfiles.manage.wouldRemoveFile',
  'configProfiles.manage.noChanges',
  'configProfiles.manage.applied',
  'configProfiles.manage.restored',
  'configProfiles.manage.lastApplied',
  'configProfiles.manage.neverApplied',
  'configProfiles.manage.hasRecovery',
  'configProfiles.manage.column.before',
  'configProfiles.manage.column.after',
  'configProfiles.manage.status.pending',
  'configProfiles.manage.status.clean',
  'configProfiles.manage.status.failed',
  'configProfiles.error.profile_not_found',
  'configProfiles.error.project_not_found',
  'configProfiles.error.invalid_profile_entry',
  'configProfiles.error.stale_profile',
  'configProfiles.error.profile_in_use',
  'configProfiles.error.library_offline',
  'configProfiles.error.source_invalid',
  'configProfiles.error.unsupported_symlink',
  'configProfiles.error.too_large',
  'configProfiles.error.stale_preview',
  'configProfiles.error.preview_expired',
  'configProfiles.error.write_failed',
  'configProfiles.error.atomic_replace_unsupported',
  'configProfiles.error.rollback_failed',
  'configProfiles.error.recovery_not_found',
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
// Every stable backend code has a message, and no message repeats a path.
for (const [name, locale] of [
  ['en', en],
  ['zh-TW', zhTW],
]) {
  const messages = locale?.configProfiles?.error ?? {};
  for (const [code, message] of Object.entries(messages)) {
    require(
      typeof message === 'string' && !message.includes('/') && !message.includes('\\'),
      `${name}.json message for ${code} must not contain a path`
    );
  }
}

if (errors.length) {
  console.error(`Config Profile management check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Config Profile management check passed.');
