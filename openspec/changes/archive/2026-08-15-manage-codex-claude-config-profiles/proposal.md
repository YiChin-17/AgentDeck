## Why

Phase 6 已能安全檢視 Codex 與 Claude Code 的固定設定來源，但使用者仍無法把同一組非敏感設定重複套用到已登錄專案，也沒有 preview、衝突保護與 restore。下一步需要把唯讀 inventory 連接到受控的 ConfigProfile persistence 與 project-scope mutation，且不得擴大到 secret、任意路徑或背景自動寫入。

## What Changes

- 建立 `config_profile` Artifact detail，以名稱、revision 與既有 inspection allowlist 內的 Agent-specific typed scalar 保存 reusable profile；建立已登錄 Project 與 Agent 的 assignment。
- 提供 profile create／edit／delete 與 assignment commands，使用 SQLite referential integrity，拒絕會留下 dangling assignment 或 recovery state 的刪除。
- 對 Codex project `.codex/config.toml` 與 Claude Code project `.claude/settings.json` 產生 allowlisted typed write preview；preview 綁定 profile revision、Project、Agent、target fingerprint 與 exact mutation。
- apply 重新讀取並驗證 preview，遇到外部修改回 `stale_preview`；只修改 profile 選定的 allowlisted keys，保留未知內容與 TOML 註解／排列。
- 寫入前建立 owner-private recovery point，使用同目錄 staged file、sync 與 atomic replace；提供 conflict-safe restore preview／apply。
- 將 Config Profiles 頁面擴充為 profile CRUD、Project assignment、preview／confirm apply 與 restore，同時保留既有 inventory、source diagnostic 與 runtime limitation。
- 新增 migration、backend fault-injection、serialization、frontend static contract 與人工 GUI 驗證，證明 rollback、取消與錯誤流程不留下部分狀態或 secret。

## Non-Goals

- 不修改 user scope `~/.codex/config.toml`、`~/.claude/settings.json` 或 Claude project-local `.claude/settings.local.json`。
- 不接受 caller-supplied path、home、cwd、environment、raw TOML／JSON、任意 key 或任意 CLI arguments。
- 不保存 secret、credential、token、API key、環境變數、permission rules、Hook、MCP、Plugin、command 或 path；本 change 不建立 secret reference 或系統安全儲存整合。
- 不提供 background auto-apply、排程、watcher-triggered write、跨專案單鍵批次 mutation 或 managed policy／CLI／environment resolution。
- 不改變 Skill、Plugin、Hook、Library、Git backup protocol 或上游跨平台行為；不新增 production dependency。

## Capabilities

### New Capabilities

- `config-profile-management`: ConfigProfile persistence、Project assignment、typed preview、conflict-safe atomic apply、recovery point 與 restore。

### Modified Capabilities

- `config-profile-inspection`: Config Profiles 頁面在保留唯讀 inventory 邊界下，新增明確分離的受控 profile management actions。
- `product-board-interface`: Config Profiles navigation 從 inspection-only 頁面改為已實作的 inspection 與 management 頁面，仍不得導向空白或未實作流程。

## Impact

- Affected phase: `plan.md` Phase 6 Config Profiles。
- Affected specs: `config-profile-management`、`config-profile-inspection`、`product-board-interface`。
- Affected code:
  - New: `src-tauri/src/core/config_profile_management.rs`、`src-tauri/src/commands/config_profile_management.rs`、`scripts/check-config-profile-management.mjs`。
  - Modified: `src-tauri/src/core/artifact.rs`、`src-tauri/src/core/config_profile_inventory.rs`、`src-tauri/src/core/migrations.rs`、`src-tauri/src/core/skill_store.rs`、`src-tauri/src/core/mod.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`、`src/lib/tauri.ts`、`src/views/ConfigProfiles.tsx`、`src/i18n/en.json`、`src/i18n/zh-TW.json`、`package.json`、`plan.md`。
  - Removed: none.
- Storage: SQLite 新增 ConfigProfile detail、entry 與 latest recovery-point tables；Project assignment 沿用 canonical deployment rows，source backup bytes 保留在 owner-private app state，不進 central Library 或 Git backup metadata。
- Dependencies: 沿用 Rust standard library、SQLite、`serde_json`、`toml_edit`、Tauri、React 與 TypeScript，不新增 production dependency。
- Compatibility: 保留既有 inventory request／response 與唯讀 route 行為；新增 commands 與 schema migration，不改上游持久協議名稱。
