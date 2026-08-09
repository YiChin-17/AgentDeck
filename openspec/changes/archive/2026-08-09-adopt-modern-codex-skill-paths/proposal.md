## Why

這是 plan.md 的 Phase 1。AgentDeck 目前將 Codex 的 `.codex/skills` 當作部署預設，與產品已確認的 `.agents/skills` 新預設不一致，也會讓同一個 Skill 同時存在於新舊路徑時重複出現在掃描結果。

## What Changes

- 將 Codex 的使用者層級與專案層級部署預設改為 `.agents/skills`。
- 保留 `.codex/skills` 作為使用者層級與專案層級的 discovery-only legacy 路徑。
- 對指向相同實體目錄或相同 Skill 內容的新舊掃描結果去重，同時保留實際來源位置資訊。
- 保留既有 global absolute path override 與 project-relative path override；override 決定部署目標，legacy 路徑只參與掃描。
- 新增 adapter、全域掃描、專案掃描與 override 優先順序的 regression tests。

## Non-Goals

- 不包含 Phase 1 的 Library offline 防護；該功能涉及中央 Library、同步與刪除保護，將由獨立 change 處理。
- 不修改 Claude Code 或其他 agent 的預設路徑。
- 不搬移、刪除或自動改寫使用者既有 `.codex/skills` 內容。
- 不改變 Skill 的 symlink／copy 部署模式，也不建立新的設定介面。
- 不擴充 Agent Skills 畫面的 global local Skill 列表與讀取／匯入／更新／刪除 commands；該流程目前只以 primary root 加 `relative_path` 定位，legacy root 顯示與同名來源識別將由 `support-legacy-codex-skills-in-agent-workspace` 獨立處理。
- 不處理 Plugins、Hooks 或 Config Profiles。

## Capabilities

### New Capabilities

- `codex-skill-path-routing`: 定義 Codex 新舊使用者與專案 Skill 路徑的部署、掃描、去重及使用者 override 行為。

### Modified Capabilities

(none)

## Impact

- Affected specs: `codex-skill-path-routing`
- Intentional upstream divergence: Codex deployment defaults change from `.codex/skills` to `.agents/skills`; legacy discovery remains compatible.
- Affected code:
  - Modified: `src-tauri/src/core/tool_adapters.rs`
  - Modified: `src-tauri/src/core/scanner.rs`
  - Modified: `src-tauri/src/core/project_scanner.rs`
  - Modified: `src-tauri/src/commands/projects.rs`
- Dependencies: no new package or crate dependency.
