## Why

Phase 4 的唯讀 Hook inspection 已能安全顯示 Codex 與 Claude Code 的實際來源，但使用者仍必須離開 AgentDeck 手動修改 JSON／TOML，無法在寫入前確認 schema、實際 diff、外部修改衝突與回復路徑。下一階段要在既有固定 source id 與 linked Project 邊界內加入可驗證、可預覽、可回復的安全編輯流程。

## What Changes

- 以既有 Hook source id 與 optional Project id 選定寫入目標，backend 重新解析固定來源；frontend 不得提交任意 filesystem path。
- 新增 Codex／Claude Code 分開的 event、matcher、handler 表單與 schema validation；只允許 registry 明確支援的可寫結構，未知既有欄位仍在 round-trip 中保留。
- 新增 write preview，回傳完整目標來源的 before／after Hook subtree diff、base content hash 與 validation 結果；不通過 validation 時不得進入 apply。
- apply 時重新比對 base hash，來源在 preview 後有外部修改即回傳 typed conflict；通過後先建立 recovery backup，再使用同目錄 temporary file 與 atomic replacement 寫入。
- 新增最近一次 AgentDeck Hook 寫入的 restore 流程；restore 同樣先檢查目前來源 hash、預覽差異並產生新的 recovery backup，避免覆蓋未確認的外部修改。
- 為受管理 Hook source 建立 kind hook 的 Artifact identity 與不含 Hook payload 的 detail／backup metadata；Hook command、prompt、URL、headers 與其他內容不得寫入 SQLite、Library、logs、localStorage 或 Git backup。
- 將 Hooks 頁面從唯讀 Inspector 擴充為明確的 Edit、Preview、Apply 與 Restore 流程，保留既有 inspection、filters、diagnostics、comparison 與 compatibility matrix。
- 新增 Rust round-trip、schema、conflict、backup／restore、atomic failure、database migration tests，以及 frontend 靜態契約、build、lint 與雙語文案驗證。

## Non-Goals

- 不執行、試跑、enable／disable Hook，也不呼叫 Hook 內的 command、prompt、HTTP endpoint、MCP tool 或 agent handler。
- 不提供跨 Agent schema conversion、跨來源 merge、批次套用或 managed policy／Plugin-bundled／component Hook 編輯。
- 不把 Hook payload 納入中央 Library 或 `.skills-manager` Git backup，不升級既有 merge protocol 2。
- 不新增 Plugin 或 Config Profile 功能，不重構既有 Skill deployment 與 backup 行為。

## Capabilities

### New Capabilities

- `hook-management`: 定義固定來源上的 Agent-specific Hook 編輯、validation、preview、optimistic conflict detection、recovery backup、atomic write、restore 與非敏感 identity metadata。

### Modified Capabilities

- `hook-inspection`: 將 Hooks 頁面從禁止 mutation controls 的唯讀介面，擴充為只有在 validation 與 preview 成功後才可進入 apply／restore 的安全操作介面。

## Impact

- Plan phase: `plan.md` Phase 4「Hooks」第二段。
- Affected specs: 新增 `hook-management`；修改 `hook-inspection`；沿用 `artifact-foundation` 的 typed Artifact 與 deployment constraints，不修改 Skill requirements。
- Intentional upstream divergence: AgentDeck 提供 Codex／Claude Code Hook 的本機 GUI 編輯與 recovery backup；官方 Plugin cache、Agent CLI、既有 Skill UI、Library 與 Git backup protocol 保持不變。
- Affected code:
  - New: `src-tauri/src/core/hook_management.rs`
  - New: `src/components/HookEditor.tsx`
  - Modified: `src-tauri/src/core/hook_inspection.rs`
  - Modified: `src-tauri/src/core/artifact.rs`
  - Modified: `src-tauri/src/core/skill_store.rs`
  - Modified: `src-tauri/src/core/migrations.rs`
  - Modified: `src-tauri/src/core/mod.rs`
  - Modified: `src-tauri/src/commands/hooks.rs`
  - Modified: `src-tauri/src/lib.rs`
  - Modified: `src/lib/tauri.ts`
  - Modified: `src/views/Hooks.tsx`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - Modified: `scripts/check-hooks-ui.mjs`
  - Modified: `package.json`
  - Modified: `plan.md`
- Dependencies: 沿用既有 `serde_json`、`toml_edit`、SQLite 與標準函式庫 filesystem primitives；不新增 runtime dependency。
