#!/usr/bin/env node
// Static contract for the Hooks page.
//
// The page has no frontend test framework, so the wiring that keeps editing
// preview-gated, keeps Hook content out of persistent state and keeps the diff
// bounded is asserted against the source itself: a regression here would
// otherwise only show up as a running Hook, a lost config or a frozen UI. Uses
// only the Node standard library.
import fs from 'node:fs';
import path from 'node:path';
import { wrapperArgumentSurface } from './frontend-argument-surface.mjs';

const root = process.cwd();
const read = (...parts) => fs.readFileSync(path.join(root, ...parts), 'utf8');

const app = read('src', 'App.tsx');
const sidebar = read('src', 'components', 'Sidebar.tsx');
const api = read('src', 'lib', 'tauri.ts');
const view = read('src', 'views', 'Hooks.tsx');
const inspector = read('src', 'components', 'HookInspector.tsx');
const editor = read('src', 'components', 'HookEditor.tsx');
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

// ── The five Hook write commands, and nothing else ──
const WRITE_COMMANDS = [
  ['previewHookChange', 'preview_hook_change'],
  ['applyHookChange', 'apply_hook_change'],
  ['getHookRecovery', 'get_hook_recovery'],
  ['previewHookRestore', 'preview_hook_restore'],
  ['applyHookRestore', 'apply_hook_restore'],
];
for (const [wrapper, command] of WRITE_COMMANDS) {
  require(
    api.includes(`export const ${wrapper} =`) && api.includes(`"${command}"`),
    `tauri.ts must expose ${command} through the ${wrapper} wrapper`
  );
  require(
    editor.includes(`api.${wrapper}(`),
    `the Hook editor must call ${command} through its wrapper`
  );
}
// Only the Hook wrappers are in scope here; unrelated Skill commands do take
// paths the user picked themselves, and so do declarations that merely happen
// to sit after the Hook block in the same file.
const hookSurface = wrapperArgumentSurface(api, [
  'getHookInspection',
  ...WRITE_COMMANDS.map(([wrapper]) => wrapper),
]);
require(
  hookSurface.missing.length === 0,
  `tauri.ts must declare every Hook wrapper (missing: ${hookSurface.missing.join(', ')})`
);
require(
  !/Path\b/.test(hookSurface.surface),
  'no Hook command may take a filesystem path from the frontend'
);

// ── No execution surface ──
const FORBIDDEN_CALLS = [
  'api.installLocal',
  'api.deleteManagedSkill',
  'api.exportSkillToProject',
  'api.toggleProjectSkill',
  'api.gitBackup',
  'writeTextFile',
  'invoke(',
  'Command(',
  'shell',
];
for (const call of FORBIDDEN_CALLS) {
  require(!view.includes(call), `the Hooks view must not call ${call}`);
  require(!editor.includes(call), `the Hook editor must not call ${call}`);
}
for (const label of ['Execute', 'Test run', 'runHook', 'enableHook', 'disableHook']) {
  require(!view.includes(label), `the Hooks page must not offer ${label}`);
  require(!editor.includes(label), `the Hook editor must not offer ${label}`);
}
require(
  view.includes('t("hooks.gatedEditBadge")'),
  'the Hooks page must label editing as preview-gated'
);

// ── Apply is impossible before a current successful preview ──
require(
  editor.includes('"editing" | "previewing" | "preview_ready" | "applying" | "applied"'),
  'the editor must model the draft-to-apply states explicitly'
);
require(
  editor.includes('const canApply =') &&
    editor.includes('state === "preview_ready" && preview !== null && preview.canApply'),
  'Apply must require a current preview the backend marked applicable'
);
require(
  editor.includes('if (!preview || !preview.canApply) return;') &&
    editor.includes('preview.baseRevision'),
  'Apply must submit the base revision of the preview it was enabled by'
);
require(
  editor.includes('const invalidate =') &&
    editor.includes('setPreview(null);') &&
    editor.includes('setState("editing");'),
  'any draft change must invalidate the earlier preview'
);
const invalidatedControls = (editor.match(/invalidate\(\);/g) ?? []).length;
require(
  invalidatedControls >= 5,
  'every editable control must invalidate the preview, not just some of them'
);

// ── Route-local sensitive state ──
require(
  !editor.includes('localStorage.') && !view.includes('localStorage.'),
  'no Hook draft, preview or recovery selection may reach localStorage'
);
require(
  !editor.includes('useApp()') && !editor.includes('AppContext'),
  'no Hook content may reach the shared app context'
);
require(
  view.includes('useEffect(() => {\n    setEditor(null);\n  }, [projectId]);'),
  'switching Project must clear the editor state'
);

// ── Source capability gates ──
require(
  view.includes('function editRefusal(source: HookSource)') &&
    view.includes('source.status === "valid" || source.status === "missing"'),
  'only a valid or missing fixed source may expose mutation controls'
);
require(
  view.includes('t(`hooks.edit.refusal.${editRefusal(source)}`)'),
  'a source that cannot be written must state why in localized text'
);
require(
  view.includes('isWritable(entry.sourceId)') && view.includes('t("hooks.edit.edit")'),
  'the Edit control must be limited to handlers in a writable source'
);
require(
  view.includes('t("hooks.edit.createFirst")'),
  'a missing writable source must offer to create its first handler'
);

// ── Unknown values are read-only ──
require(
  editor.includes('field.known') && editor.includes('t("hooks.edit.unknownFields")'),
  'existing unknown values must be shown read-only, never as editable controls'
);

// ── Restore is previewed ──
require(
  editor.includes('const runRestorePreview =') &&
    editor.includes('!restorePreview || !restorePreview.canApply'),
  'Restore must require a current restore preview'
);
require(
  editor.includes('restorePreview.baseRevision'),
  'Restore must submit the current base revision'
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
for (const [name, text] of [['view', view], ['editor', editor]]) {
  require(
    text.includes('requestIdRef') &&
      text.includes('if (requestIdRef.current !== requestId) return;'),
    `the Hooks ${name} must drop responses from superseded requests`
  );
}

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
  'hooks.gatedEditBadge',
  'hooks.edit.preview',
  'hooks.edit.apply',
  'hooks.edit.restoreApply',
  'hooks.edit.unknownFields',
  'hooks.edit.refusal.too_large',
  'hooks.edit.issue.unknown_handler_type',
  'hooks.edit.error.source_conflict',
  'hooks.edit.error.recovery_required',
  'hooks.edit.error.atomic_replace_unsupported',
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
