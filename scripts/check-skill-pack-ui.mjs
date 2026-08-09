#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const source = fs.readFileSync(path.join(root, 'src', 'components', 'PresetBar.tsx'), 'utf8');
const errors = [];

if (!source.includes('t("presetBar.add")') || !source.includes('t("presetBar.remove")')) {
  errors.push('each non-empty Skill Pack must expose separately labeled add and remove actions');
}
if (source.includes('if (s.status === "active") handleDeactivate(preset)')) {
  errors.push('the Skill Pack label must not toggle between add and remove mutations');
}
if (!source.includes('setPendingRemoval({ preset, count: s.installed })')) {
  errors.push('remove must stage the exact matching Skill-Agent count before mutation');
}
if (!source.includes('<ConfirmDialog') ||
    !source.includes('name: pendingRemoval.preset.name') ||
    !source.includes('count: pendingRemoval.count')) {
  errors.push('remove confirmation must identify the Skill Pack and exact matching count');
}
if (!source.includes('onClose={() => setPendingRemoval(null)}') ||
    !source.includes('onConfirm={() => handleDeactivate(pendingRemoval.preset)}')) {
  errors.push('cancel must close confirmation without calling the removal handler');
}
if (!source.includes('t("presetBar.removeKeepsLibrary")')) {
  errors.push('remove confirmation must state that central Skills and membership are unchanged');
}

if (errors.length) {
  console.error(`Skill Pack UI check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Skill Pack UI check passed.');
