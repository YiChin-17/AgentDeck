#!/usr/bin/env node
// Static contract for user-scope Plugin mutation.
//
// The page has no frontend test framework, so the wiring that keeps a mutation
// reviewable is asserted against the source itself: a regression here would
// otherwise only show up as a Plugin installed, removed or disabled in the
// user's real CLI without a confirmation. Uses only the Node standard library.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const errors = [];
const require = (condition, message) => {
  if (!condition) errors.push(message);
};

// A missing file is a contract failure, not a crash: this check runs before the
// mutation UI exists and has to report what is missing.
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
const view = read('src', 'views', 'Plugins.tsx');
const rust = read('src-tauri', 'src', 'commands', 'plugin_mutation.rs');
const lib = read('src-tauri', 'src', 'lib.rs');
const pkg = JSON.parse(read('package.json') || '{}');
const en = JSON.parse(read('src', 'i18n', 'en.json') || '{}');
const zhTW = JSON.parse(read('src', 'i18n', 'zh-TW.json') || '{}');

const OPERATIONS = ['install', 'update', 'remove', 'enable', 'disable'];
const DIAGNOSTIC_CODES = [
  'operation_unsupported',
  'identity_not_found',
  'scope_unsupported',
  'precondition_failed',
  'preview_expired',
  'stale_preview',
  'interactive_confirmation_required',
  'verification_failed',
  'cli_missing',
  'timeout',
  'non_zero_exit',
  'output_too_large',
];

// ── Two commands, registered, and nothing else ──
require(
  pkg.scripts?.['check:plugin-mutations'] === 'node scripts/check-plugin-mutations.mjs',
  'package.json must expose this contract as check:plugin-mutations'
);
require(
  lib.includes('commands::plugin_mutation::preview_plugin_mutation') &&
    lib.includes('commands::plugin_mutation::apply_plugin_mutation'),
  'lib.rs must register both mutation commands'
);
require(
  lib.includes('PluginMutationState::default()') && lib.includes('.manage('),
  'lib.rs must manage the in-memory preview state'
);

// ── The request carries an intent, never a process ──
require(
  api.includes('invoke<PluginMutationPreviewOutcome>("preview_plugin_mutation"'),
  'tauri.ts must call preview_plugin_mutation'
);
require(
  api.includes('invoke<PluginMutationApplyOutcome>("apply_plugin_mutation"'),
  'tauri.ts must call apply_plugin_mutation'
);
// The preview response carries a scope and an argv display for the user to
// read, so the guarantee has to be about the request shape, not about which
// words appear nearby: these four fields are the whole request.
const previewRequest = api.match(/previewPluginMutation\s*=\s*\(request:\s*\{([^}]*)\}\)/);
require(previewRequest !== null, 'previewPluginMutation must take one request object');
const previewFields = (previewRequest?.[1] ?? '')
  .split(';')
  .map((field) => field.trim().split(':')[0].trim())
  .filter(Boolean)
  .sort();
require(
  JSON.stringify(previewFields) === JSON.stringify(['agent', 'marketplace', 'operation', 'pluginId']),
  `the preview request must carry only the Agent, operation and identity, got ${previewFields.join(', ')}`
);
require(
  /applyPluginMutation\s*=\s*\(token: string\)/.test(api),
  'applyPluginMutation must take the token and nothing else'
);
require(
  /apply_plugin_mutation",\s*\{\s*request:\s*\{\s*token\s*\}\s*\}/.test(api),
  'the apply request body must be the token alone'
);

// ── The matrix comes from the backend ──
require(
  /mutations:\s*PluginOperationKey\[\]/.test(api),
  'PluginAgentInventory must carry the backend mutation capability matrix'
);
require(
  rust.includes('PluginMutationState') && !rust.includes('Store'),
  'the mutation command must take the preview state and no store or Library path'
);
require(
  view.includes('.mutations.includes('),
  'the page must decide availability from the backend matrix, not from the Agent name'
);
require(
  !/agent(Key)?\s*===\s*"codex"\s*\?/.test(view),
  'the page must not branch on an Agent name to decide what it can mutate'
);

// ── Preview first, then a token-only apply ──
require(
  view.includes('api.previewPluginMutation(') && view.includes('api.applyPluginMutation('),
  'the page must call preview and apply through the typed wrappers'
);
require(
  /api\.applyPluginMutation\(\s*pending\.token\s*\)/.test(view),
  'apply must send the token from the preview the user confirmed'
);
require(
  !/api\.applyPluginMutation\((?!\s*pending\.token\s*\))/.test(view),
  'no apply may be issued from anything other than the pending preview'
);
require(
  view.includes('outcome.status === "ready"'),
  'the page must branch on the preview outcome instead of assuming success'
);

// ── Every operation is gated and every refusal is explained ──
for (const operation of OPERATIONS) {
  require(
    view.includes(`"${operation}"`),
    `the page must offer the ${operation} operation from the normalized enum`
  );
}
require(
  view.includes('disabledReason') && view.includes('disabled={'),
  'unavailable operations must render disabled with a computed reason'
);
require(
  view.includes('t(`plugins.mutation.disabled.${') &&
    ['"unsupported"', '"scope"', '"precondition"'].every((reason) => view.includes(reason)),
  'a disabled operation must show which of the three fixed reasons blocked it'
);
require(
  /scope\s*!==\s*"user"/.test(view),
  'Claude Code mutation must be disabled outside user scope'
);
require(
  view.includes('item.agent === "claude_code"'),
  'only Claude Code records may be gated on their inventory scope'
);
require(
  view.includes('operation === "install"') && view.includes('scope === "unknown"'),
  'Claude Code install must be reachable for available-only unknown-scope records'
);

// ── Destructive removal is confirmed against the preview ──
require(
  view.includes('pending.destructive'),
  'the destructive confirmation must come from the preview, not from the click'
);
for (const field of ['pending.agent', 'pending.pluginId', 'pending.marketplace', 'pending.scope', 'pending.argvDisplay']) {
  require(
    view.includes(field),
    `the confirmation must display ${field} from the same preview`
  );
}
require(
  view.includes('mutation.confirm.cancel') && /setPending\(null\)/.test(view),
  'cancel must clear the pending preview and send nothing'
);

// ── Verified inventory replaces the page's data; failure refreshes it ──
require(
  /outcome\.status === "verified"/.test(view) && /setData\(outcome\.inventory\)/.test(view),
  'a verified mutation must replace route-local data with the returned inventory'
);
require(
  /outcome\.status === "verified"[\s\S]{0,600}void load\(\)/.test(view) ||
    /status === "failed"[\s\S]{0,400}void load\(\)/.test(view),
  'a failed mutation must trigger a fresh inventory request'
);
require(
  view.includes('t(`plugins.mutation.diagnostic.'),
  'a mutation failure must render a localized fixed diagnostic'
);

// ── The token stays in this route ──
require(!view.includes('localStorage.'), 'no preview token may reach localStorage');
require(
  !view.includes('useApp()') && !view.includes('AppContext'),
  'no preview token may reach the shared app context'
);
require(
  !/sessionStorage|window\.__/.test(view),
  'no preview token may be parked on a global'
);

// ── Nothing the change explicitly refused ──
for (const forbidden of [
  '"-y"',
  '--yes',
  '--config',
  '--keep-data',
  '--prune',
  '--all',
  'marketplaceAdd',
  'addMarketplace',
  'removeMarketplace',
  'validatePlugin',
  'pluginDetails',
  'runEval',
  'prunePlugin',
]) {
  for (const [name, source] of [
    ['the Plugins view', view],
    ['tauri.ts', api],
  ]) {
    require(!source.includes(forbidden), `${name} must not offer ${forbidden}`);
  }
}

// ── Bilingual coverage ──
const REQUIRED_KEYS = [
  'plugins.mutation.heading',
  'plugins.mutation.applying',
  'plugins.mutation.failed',
  'plugins.mutation.verified',
  'plugins.mutation.confirm.title',
  'plugins.mutation.confirm.destructiveTitle',
  'plugins.mutation.confirm.warning',
  'plugins.mutation.confirm.agent',
  'plugins.mutation.confirm.plugin',
  'plugins.mutation.confirm.marketplace',
  'plugins.mutation.confirm.scope',
  'plugins.mutation.confirm.command',
  'plugins.mutation.confirm.apply',
  'plugins.mutation.confirm.cancel',
  'plugins.mutation.disabled.unsupported',
  'plugins.mutation.disabled.scope',
  'plugins.mutation.disabled.precondition',
  ...OPERATIONS.map((operation) => `plugins.mutation.operation.${operation}`),
  ...DIAGNOSTIC_CODES.map((code) => `plugins.mutation.diagnostic.${code}`),
];
const valueAt = (locale, key) =>
  key.split('.').reduce((node, part) => (node ? node[part] : undefined), locale);
for (const key of REQUIRED_KEYS) {
  for (const [name, locale] of [
    ['en', en],
    ['zh-TW', zhTW],
  ]) {
    const value = valueAt(locale, key);
    require(typeof value === 'string' && value.length > 0, `${name}.json is missing ${key}`);
  }
}
// Parity is stricter than presence: a key that exists in one locale only would
// render as a raw key path for half the users.
const flatten = (node, prefix = '') =>
  Object.entries(node ?? {}).flatMap(([key, value]) =>
    value && typeof value === 'object'
      ? flatten(value, `${prefix}${key}.`)
      : [`${prefix}${key}`]
  );
const enMutationKeys = flatten(valueAt(en, 'plugins.mutation')).sort();
const zhMutationKeys = flatten(valueAt(zhTW, 'plugins.mutation')).sort();
require(
  JSON.stringify(enMutationKeys) === JSON.stringify(zhMutationKeys),
  'plugins.mutation keys must match exactly between en and zh-TW'
);

if (errors.length) {
  console.error(`Plugin mutation check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Plugin mutation check passed.');
