import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), "utf8");

test("window, HTML, Tray, Settings diagnostics and README identify AgentDeck", () => {
  const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
  const html = read("index.html");
  const rust = read("src-tauri/src/lib.rs");
  const settingsCommand = read("src-tauri/src/commands/settings.rs");
  const settingsView = read("src/views/Settings.tsx");
  const readme = read("README.md");

  assert.equal(tauri.app.windows[0].title, "AgentDeck", "main window title");
  assert.match(html, /<title>AgentDeck<\/title>/);
  assert.doesNotMatch(rust, /Skills Manager/);
  assert.match(rust, /"AgentDeck · \{\} skills · \{\} agents/);
  assert.match(rust, /"tray-app-name", "AgentDeck"/);
  assert.match(rust, /"Open AgentDeck"/);
  assert.match(settingsCommand, /# AgentDeck Diagnostics/);
  assert.match(settingsView, /auto-collected by AgentDeck/);
  assert.match(readme, /<h1 align="center">AgentDeck<\/h1>/);
});

test("README and plan document the product identity and legacy compatibility boundary", () => {
  const readme = read("README.md");
  const plan = read("plan.md");
  const baseline = read("BASELINE.md");
  const license = read("LICENSE");

  for (const document of [readme, plan]) {
    assert.match(document, /io\.github\.yichin17\.agentdeck/);
    assert.match(document, /\.skills-manager/);
    assert.match(document, /skills-manager\.db/);
    assert.match(document, /skills-manager-cli/);
  }
  assert.match(readme, /GitHub.*skills-manager/s);
  assert.match(readme, /After confirming AgentDeck.*manually remove.*Skills Manager\.app/s);
  assert.match(baseline, /upstream.*xingkongliang\/skills-manager\.git/s);
  assert.match(license, /^MIT License/);
});

for (const locale of ["en", "zh-TW"]) {
  test(`${locale} App-owned product strings identify AgentDeck`, () => {
    const messages = JSON.parse(read(`src/i18n/${locale}.json`));
    const ownedStrings = [
      messages.app.name,
      messages.help.title,
      messages.help.items.global.description,
      messages.settings.version,
      messages.settings.panicBanner,
    ];

    assert.equal(messages.app.name, "AgentDeck");
    for (const value of ownedStrings) {
      assert.match(value, /AgentDeck/);
      assert.doesNotMatch(value, /Skills Manager/);
    }

    assert.match(messages.backup.disconnect.revokeConfirmOauth, /skills-manager/);
    assert.match(messages.settings.repoWarning_config_unreadable, /~\/\.skills-manager/);
  });
}
