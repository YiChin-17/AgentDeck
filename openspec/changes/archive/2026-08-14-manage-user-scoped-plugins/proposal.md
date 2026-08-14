## Why

Phase 5 已建立可信任的唯讀 Plugin inventory，但使用者仍必須離開 AgentDeck 才能安裝或變更 Plugin 狀態。Codex 0.144.5 與 Claude Code 2.1.231 的 mutation capability 不對稱且部分命令可能要求互動確認，因此現在需要以固定 capability matrix、preview token 與 fail-closed 執行邊界加入第一批安全的 user-scope 操作。

## What Changes

- 為 Codex 加入 user-scope Plugin add／remove，為 Claude Code 加入 user-scope install／update／uninstall／enable／disable；未受 CLI help contract 支援的操作保持 unavailable。
- 新增 mutation preview 與 apply 兩階段 IPC，frontend 只傳 Agent、operation 與 inventory identity；backend 產生固定 argv、base inventory fingerprint 與一次性 token，apply 只接受完全相符且未失效的 preview。
- 延用 no-shell、closed stdin、10-second timeout、1 MiB stdout／stderr 上限、kill／reap 與 sanitized diagnostics；禁止 `-y`、`--config`、`--keep-data`、`--prune`、`--all` 及 caller-controlled executable／cwd／environment／arguments。
- apply 成功後重新收集 inventory，只有新 inventory 證明目標狀態時才回報 verified success；stale preview、互動確認需求、CLI failure 與 refresh failure 均維持 typed failure。
- 在 Plugins 頁面依 capability matrix 與 user-scope record 顯示可用操作，remove／uninstall 使用 preview 內容二次確認；新增 Rust tests、frontend static contract、production build、lint 與雙語文案驗證。
- 這是 `plan.md` Phase 5 的第二個 change；保留既有 Skill、Hook、backup、唯讀 Plugin inventory 與跨平台行為，沒有 intentional divergence。

## Non-Goals

- 不支援 project／local／managed scope mutation、Project-specific Plugin 指派或 caller-controlled cwd。
- 不執行 Codex update／enable／disable，也不以 remove＋add 模擬 update。
- 不執行 marketplace add／remove／upgrade、validation、details、eval、prune、Plugin scaffolding、tag 或 Plugin payload inspection。
- 不自動傳入 `-y` 接受 marketplace 宣告的外部命令，不提供 arbitrary Plugin configuration。
- 不直接讀寫官方 Plugin cache、manifest 或 settings，不建立 Plugin Artifact、deployment、Library copy、Git backup metadata或持久化 inventory／preview token。

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `plugin-inventory`: 擴充為固定 user-scope mutation capability matrix、preview／apply token contract、verified refresh、typed failure 與 Plugins 頁面安全操作。

## Impact

- Affected specs: modified `plugin-inventory`
- Affected code:
  - New: `src-tauri/src/core/plugin_mutation.rs`, `src-tauri/src/commands/plugin_mutation.rs`, `scripts/check-plugin-mutations.mjs`
  - Modified: `src-tauri/src/core/mod.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/core/plugin_inventory.rs`, `src/lib/tauri.ts`, `src/views/Plugins.tsx`, `src/i18n/en.json`, `src/i18n/zh-TW.json`, `package.json`
  - Removed: none
- Runtime boundary: installed Codex and Claude Code executables, invoked without a shell through backend-owned user-scope mutation arguments
- Persistence: no schema migration and no new dependency; preview state stays in bounded backend memory and Plugin state remains owned by official CLIs
