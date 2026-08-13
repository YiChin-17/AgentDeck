## Why

Phase 5 需要先建立可信任的 Plugin 現況基線。Codex 與 Claude Code 都由各自官方 CLI 管理 Plugin 與 marketplace，但輸出欄位、可用操作與失敗方式不同；AgentDeck 目前沒有安全的唯讀 adapter 或統一頁面，無法在不碰官方 cache 的前提下顯示 installed、available、版本與來源狀態。

## What Changes

- 建立 Codex 與 Claude Code 分開的唯讀 Plugin CLI adapters，只允許固定 executable 與 allowlist 參數組合，取得 CLI version、Plugin JSON inventory 與 marketplace JSON inventory。
- 將兩個 Agent 的 JSON 正規化為 AgentDeck Plugin DTO，保留 Agent、Plugin id、display name、installed／available、version、scope、marketplace、enabled 與 update 狀態；CLI 未提供的欄位明確標記 unknown，不跨 Agent 推測。
- 為 missing CLI、unsupported CLI contract、timeout、non-zero exit、invalid JSON、oversized output 與 marketplace unavailable 建立 sanitized diagnostics；單一 Agent 失敗不阻斷另一個 Agent。
- 新增唯讀 Plugins 頁面與 Sidebar route，提供 Agent、installed／available、scope、marketplace 與狀態 filters，以及來源診斷與 Plugin details。
- 新增 Rust adapter／parser／command tests、frontend static contract、production build、lint 與雙語文案驗證。
- 這是 `plan.md` Phase 5 的第一個 change；保留上游 Skill、Hook、backup 與跨平台行為，沒有 intentional divergence。

## Non-Goals

- 不執行 install、update、remove、enable、disable、marketplace mutation、validation 或 Plugin eval。
- 不直接讀寫 Codex／Claude Code 官方 Plugin cache、manifest、settings 或 marketplace 檔案，不掃描 Plugin bundle 內的 Skills、Hooks、MCP servers、scripts 或 dependencies。
- 不建立 Plugin Artifact、detail、deployment、Library copy 或 Git backup metadata；inventory 只存在於當次 backend response 與 route-local UI state。
- 不新增 Project-specific Plugin 指派、Board lanes、跨 Agent conversion、版本解析推測或自動修復。

## Capabilities

### New Capabilities

- `plugin-inventory`: 定義固定官方 CLI 上的唯讀 capability detection、bounded JSON inventory、Agent-specific normalization、隔離診斷、敏感資料邊界與 Plugins 頁面。

### Modified Capabilities

(none)

## Impact

- Affected specs: new `plugin-inventory`
- Affected code:
  - New: `src-tauri/src/core/plugin_inventory.rs`, `src-tauri/src/commands/plugins.rs`, `src/views/Plugins.tsx`, `scripts/check-plugins-ui.mjs`
  - Modified: `src-tauri/src/core/mod.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri.ts`, `src/App.tsx`, `src/components/Sidebar.tsx`, `src/i18n/en.json`, `src/i18n/zh-TW.json`, `package.json`
  - Removed: none
- Runtime boundary: installed Codex and Claude Code executables, invoked without a shell through fixed read-only arguments
- Dependencies: no new dependency; reuse Tokio process and timeout support already present in `src-tauri/Cargo.toml`
