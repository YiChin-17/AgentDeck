import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { wrapperArgumentSurface } from "./frontend-argument-surface.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const api = fs.readFileSync(path.join(root, "src", "lib", "tauri.ts"), "utf8");

const HOOK_WRAPPERS = [
  "getHookInspection",
  "previewHookChange",
  "applyHookChange",
  "getHookRecovery",
  "previewHookRestore",
  "applyHookRestore",
];
const PLUGIN_WRAPPERS = ["getPluginInventory", "previewPluginMutation", "applyPluginMutation"];
const FORBIDDEN_PLUGIN_ARGUMENTS = ["Path", "executable", "args", "cwd", "env"];

/// A miniature `tauri.ts`: two Hook wrappers, then unrelated declarations that
/// were added later. The trailing block is what a whole-file scan mistakes for
/// a Hook argument — `displayPath` is a backend response field, and the comment
/// merely contains the word "environment".
const SOURCE_WITH_LATER_DECLARATIONS = `
export const previewHookChange = (projectId: string, draft: string) =>
  invoke<HookPreview>("preview_hook_change", { projectId, draft });

export const applyHookChange = (token: string) =>
  invoke<HookApplyOutcome>("apply_hook_change", { token });

/**
 * The request names a registered Project, so the frontend cannot point the
 * backend at a directory, a file or an environment.
 */
export interface ConfigSource {
  displayPath: string;
}

export const getConfigProfileInventory = (request: ConfigInventoryRequest) =>
  invoke<ConfigProfileInventory>("get_config_profile_inventory", { request });
`;

test("argument surface stops at the end of each named wrapper", () => {
  const { surface, missing } = wrapperArgumentSurface(SOURCE_WITH_LATER_DECLARATIONS, [
    "previewHookChange",
    "applyHookChange",
  ]);

  assert.deepEqual(missing, []);
  assert.match(surface, /preview_hook_change/);
  assert.match(surface, /apply_hook_change/);
  assert.doesNotMatch(surface, /displayPath/);
  assert.doesNotMatch(surface, /environment/);
});

test("a filesystem path added to a wrapper stays inside the argument surface", () => {
  const source = SOURCE_WITH_LATER_DECLARATIONS.replace(
    "(projectId: string, draft: string)",
    "(hookPath: string, draft: string)",
  );

  const { surface } = wrapperArgumentSurface(source, ["previewHookChange"]);

  assert.match(surface, /Path\b/);
});

test("each forbidden Plugin argument stays inside the argument surface", () => {
  for (const forbidden of FORBIDDEN_PLUGIN_ARGUMENTS) {
    const source = `
export const previewPluginMutation = (request: { ${forbidden}: string }) =>
  invoke<PluginMutationPreview>("preview_plugin_mutation", { request });

export interface ConfigSource {
  displayPath: string;
}
`;

    const { surface } = wrapperArgumentSurface(source, ["previewPluginMutation"]);

    assert.ok(surface.includes(forbidden), `${forbidden} must remain visible to the rule`);
  }
});

test("a renamed or removed wrapper is reported instead of silently narrowing the surface", () => {
  const { surface, missing } = wrapperArgumentSurface(SOURCE_WITH_LATER_DECLARATIONS, [
    "previewHookChange",
    "applyHookRestore",
  ]);

  assert.deepEqual(missing, ["applyHookRestore"]);
  assert.match(surface, /preview_hook_change/);
});

test("the repository Hook wrappers take no filesystem path from the frontend", () => {
  const { surface, missing } = wrapperArgumentSurface(api, HOOK_WRAPPERS);

  assert.deepEqual(missing, []);
  assert.doesNotMatch(surface, /Path\b/);
});

test("the repository Plugin wrappers take no path, executable, argument vector, cwd or env", () => {
  const { surface, missing } = wrapperArgumentSurface(api, PLUGIN_WRAPPERS);

  assert.deepEqual(missing, []);
  for (const forbidden of FORBIDDEN_PLUGIN_ARGUMENTS) {
    assert.ok(!surface.includes(forbidden), `Plugin wrappers must not take ${forbidden}`);
  }
});
