## Why

AgentDeck 已完成 Hooks 與 Plugins，但 Config Profiles 仍沒有安全的檢視入口，使用者無法在同一介面確認 Codex TOML 與 Claude Code JSON 設定目前由哪個 scope 生效。這是 `plan.md` Phase 6 的第一個 change，先建立不寫檔、不洩漏敏感值的唯讀基礎，後續才可安全設計 profile 套用與回復。

## What Changes

- 從固定 user／project／local 路徑唯讀 discovery Codex 與 Claude Code 設定檔；project root 僅接受 AgentDeck 已登錄專案。
- 以 Agent-specific parser 解析 TOML／JSON，將明確 allowlist 內的非敏感設定正規化為共同 inventory，保留 Agent、scope、來源、存在狀態、解析狀態與 fingerprint。
- 以 typed diagnostic 隔離缺檔與格式錯誤，單一來源失敗時其餘來源仍可顯示。
- 新增 Config Profiles 頁面的 Agent、scope、project filters、來源診斷、有效值來源與唯讀差異檢視。
- 新增 backend、frontend、i18n 與 static contract 測試，確認未知與敏感欄位內容不跨過 backend boundary，也不讀寫真實使用者設定。

## Non-Goals

- 不建立或持久化 Config Profile、Artifact detail 或 deployment assignment。
- 不寫回、修復、格式化或刪除任何 Codex／Claude Code 設定檔。
- 不提供 create、assign、apply、backup、restore、secret storage 或 Git backup。
- 不掃描任意 caller-controlled path、未登錄專案或環境變數指定的位置。
- 不改變既有 Skills、Hooks、Plugins、Library、database schema 或上游跨平台行為。

## Capabilities

### New Capabilities

- `config-profile-inspection`: 固定來源的 Codex／Claude Code 設定 discovery、安全 allowlist normalization、來源診斷、有效值來源與唯讀 diff。

### Modified Capabilities

- `product-board-interface`: Config Profiles 已具備唯讀管理頁後，sidebar 可導向該頁，同時仍隱藏未實作的 Artifact workflow。

## Impact

- Backend：新增 `src-tauri/src/core/config_profile_inventory.rs`、`src-tauri/src/commands/config_profile_inventory.rs`，並更新 `src-tauri/src/core/mod.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`。
- Frontend：新增 `src/views/ConfigProfiles.tsx`，並更新 `src/App.tsx`、`src/lib/tauri.ts`、`src/components/Sidebar.tsx`、`src/i18n/en.json`、`src/i18n/zh-TW.json`。
- Contracts：新增 `scripts/check-config-profiles-ui.mjs`，並更新 `package.json`。
- Specs：新增 `openspec/specs/config-profile-inspection/spec.md`，修改 `openspec/specs/product-board-interface/spec.md`。
- Dependencies／storage：使用既有 Rust、Tauri、React、TypeScript 與 TOML／JSON 能力，不新增 production dependency，不修改 SQLite schema，不寫入 Library、設定來源或系統安全儲存。
