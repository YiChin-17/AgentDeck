import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), "utf8");

test("desktop package and Bundle metadata use the stable AgentDeck identity", () => {
  const packageJson = JSON.parse(read("package.json"));
  const packageLock = JSON.parse(read("package-lock.json"));
  const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
  const cargo = read("src-tauri/Cargo.toml");
  const cargoLock = read("src-tauri/Cargo.lock");

  assert.equal(packageJson.name, "agentdeck", "package.json npm package");
  assert.equal(packageLock.name, "agentdeck", "package-lock.json root package");
  assert.equal(packageLock.packages[""].name, "agentdeck", "package-lock.json workspace package");
  assert.equal(tauri.productName, "AgentDeck", "Tauri productName");
  assert.equal(tauri.identifier, "io.github.yichin17.agentdeck", "Tauri Bundle ID");
  assert.match(cargo, /^name = "agentdeck"$/m, "Cargo desktop package");
  assert.match(cargo, /^default-run = "agentdeck"$/m, "Cargo default desktop binary");
  assert.match(cargoLock, /\[\[package\]\]\nname = "agentdeck"\nversion = "1\.0\.0"/, "Cargo.lock package");
});

test("internal library crate and legacy Skill CLI remain explicit exceptions", () => {
  const cargo = read("src-tauri/Cargo.toml");
  const runner = read("scripts/run-rust-cli.mjs");

  assert.match(cargo, /\[lib\]\nname = "app_lib"/);
  assert.ok(fs.existsSync(path.join(root, "src-tauri/src/bin/skills-manager-cli.rs")));
  assert.match(runner, /'--bin', 'skills-manager-cli'/);
});
