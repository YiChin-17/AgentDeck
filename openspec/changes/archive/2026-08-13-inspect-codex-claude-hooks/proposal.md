## Why

AgentDeck 已有通用 Artifact identity 與 deployment 基礎，但目前無法盤點 Codex 與 Claude Code 實際載入的 Hook 設定。Phase 4 必須先建立不執行、不寫入的可信 inspection surface，讓使用者看清來源、scope、格式錯誤與跨 Agent 相容差異，再進入後續 editor 與 backup change。

## What Changes

- 新增 Codex Hook discovery：讀取 user／project scope 的 `hooks.json` 與 `config.toml` inline hooks，保留每一筆來源檔、scope、event、matcher 與 handler 資料。
- 新增 Claude Code Hook discovery：讀取 user、project 與 project-local `settings.json` 的 hooks，保留來源層與原始 handler type；不把不同來源誤當成互相覆蓋。
- 將兩個 Agent 的 Hook 設定轉成唯讀 inspection DTO，顯示已知 event／handler 支援狀態、未知欄位與來源層級 diagnostics；單一檔案錯誤不阻止其他有效來源顯示。
- 新增 Hooks 頁面與 Inspector，可依 Agent、scope、event 與來源篩選，檢視完整 command／prompt／URL 等設定，並比較同一 Agent 任兩個來源檔的文字差異。
- 加入由固定 compatibility registry 產生的 Codex／Claude Code matrix；registry 明確區分共同能力、單一 Agent 能力與目前未知項目，不將未知值自動視為支援。
- 新增 Rust parser／discovery tests、Tauri DTO contract tests、frontend 靜態契約檢查與雙語文案驗證。

## Non-Goals

- 不建立或修改 Hook Artifact detail table、deployment row 或 database migration；唯讀結果從實際設定檔即時產生。
- 不新增 Hook 表單編輯、schema rewrite、atomic write、backup／restore、enable／disable 或執行測試。
- 不修改 Plugin-bundled Hook、managed policy、Skill／agent frontmatter Hook 或官方 Plugin cache。
- 不改 `.skills-manager` metadata、Git backup schema／protocol、refs、trailers或 Keychain 資料。
- 不宣稱 Codex 與 Claude Code 的相似 event 具有相同 runtime 語意；matrix 只呈現各自解析到且 registry 明確定義的能力。

## Capabilities

### New Capabilities

- `hook-inspection`: 定義 Codex／Claude Code Hook 設定的唯讀 discovery、來源 diagnostics、檢視、同 Agent 來源 diff 與 compatibility matrix。

### Modified Capabilities

（無）

## Impact

- Plan phase: `plan.md` Phase 4「Hooks」第一段。
- Affected specs: 新增 `hook-inspection`；不修改 `artifact-foundation` 或 `product-board-interface` requirements。
- Intentional upstream divergence: AgentDeck 新增 Hooks 專用唯讀頁面與跨 Agent compatibility matrix；既有 Skill UI、commands、backup 與 Agent 設定檔保持不變。
- Affected code:
  - New: `src-tauri/src/core/hook_inspection.rs`
  - New: `src-tauri/src/commands/hooks.rs`
  - New: `src/views/Hooks.tsx`
  - New: `src/components/HookInspector.tsx`
  - New: `scripts/check-hooks-ui.mjs`
  - Modified: `src-tauri/src/core/mod.rs`
  - Modified: `src-tauri/src/commands/mod.rs`
  - Modified: `src-tauri/src/lib.rs`
  - Modified: `src-tauri/Cargo.toml`
  - Modified: `src-tauri/Cargo.lock`
  - Modified: `src/lib/tauri.ts`
  - Modified: `src/App.tsx`
  - Modified: `src/components/Sidebar.tsx`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - Modified: `package.json`
- Dependencies: 新增 Rust `toml_edit` 以解析 Codex inline hooks；沿用既有 `serde_json` 處理 JSON，不新增 frontend dependency。
