## Why

這是 plan.md Phase 1 的相容性 follow-up。Codex 部署預設改為 `.agents/skills` 後，Agent Skills 畫面的 global local Skill 流程仍只掃描 primary root，導致既有 `~/.codex/skills` Skill 在該畫面消失；若只補掃描，現有 `agent + relative_path` IPC identity 又無法區分 modern 與 legacy 的不同內容同名 Skill。

## What Changes

- Agent Skills 畫面依 adapter discovery metadata 掃描 global primary 與 additional roots，canonical root 只遍歷一次。
- 相同 agent、名稱與 content hash 的結果只顯示 precedence 較高的一筆；內容不同的同名 Skill 保留兩筆並顯示實際來源。
- 以 Skill 的已掃描實際路徑作為文件讀取與 actions 的 IPC identity，backend 每次操作前重新驗證該路徑仍屬於該 agent 的允許 roots。
- additional root 結果標記為 read-only：允許查看文件及匯入中心，但禁止直接 pull、delete 或移除其他 primary target；匯入不得改寫 legacy 來源或自動建立 global sync target。
- primary root 的既有讀取、匯入、pull、delete 與 managed target 行為維持不變；global override 若指向 legacy 目錄，該目錄視為 writable primary。
- Agent Skills 列表以實際路徑區分同名項目，顯示 read-only 來源狀態，並補齊英文、簡體中文與繁體中文文案及 regression tests。

## Non-Goals

- 不搬移、刪除、覆寫或自動改寫 `~/.codex/skills` 內容。
- 不改變 project workspace 掃描、Codex deployment target、settings schema、資料庫 schema 或 symlink／copy 策略。
- 不擴充 Plugins、Hooks、Config Profiles，也不改變其他 agent 的 primary root 行為。
- 不在本 change 自動解決 modern 與 legacy 的內容衝突；兩筆來源保持可見並由使用者決定後續處理。

## Capabilities

### New Capabilities

- `agent-workspace-discovery-roots`: 定義 Agent Skills 畫面對 primary 與 discovery-only global roots 的列表、identity、去重、read-only actions 與安全驗證行為。

### Modified Capabilities

(none)

## Impact

- Affected specs: `agent-workspace-discovery-roots`
- Related specs: `codex-skill-path-routing`（不修改其 deployment 與 scanner-based discovery requirements）
- Behavioral reach: 掃描行為依 adapter discovery metadata 套用，因此 GitHub Copilot 與 Pi 也會把 `~/.agents/skills` 顯示為 read-only additional-root 結果；兩者的 primary root 與既有 writable 行為不變。
- Affected code:
  - Modified: `src-tauri/src/commands/agent_workspace.rs`
  - Modified: `src/lib/tauri.ts`
  - Modified: `src/views/WorkspaceView.tsx`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh.json`
  - Modified: `src/i18n/zh-TW.json`
- Dependencies: no new package or crate dependency.
