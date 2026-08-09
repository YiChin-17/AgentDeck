## Why

這是 `plan.md` 的 Phase 0。AgentDeck 已建立自上游 `xingkongliang/skills-manager` 的 fork，但在開始產品功能分歧前，仍需要可重現的建置、測試與依賴安全基準，並把上游來源及實際 fork 起點記錄在專案文件中。

## What Changes

- 記錄 `upstream`、`origin`、fork 起點 commit `ab2a694` 與 tag `v1.30.0`，保留 MIT License 和必要 attribution。
- 安裝鎖定版本的前端與 Rust 依賴，執行 React／TypeScript production build、Rust tests 與現行 production dependency audits。
- 將實際指令、通過數量、失敗項目和安全 advisory 結果記錄回基準文件；不得沿用 `plan.md` 中過往研究結果冒充本次結果。
- 若 production dependency audit 發現可在不改變產品行為下修復的 advisory，更新對應 manifest／lockfile，重新執行受影響的建置、測試與 audit。
- 更新 README，清楚標示上游來源、AgentDeck 產品方向及目前仍保留的上游跨平台能力。
- 完成 Phase 0 後確認 Git working tree 僅包含預期的基準文件或必要依賴修正。

## Non-Goals

- 不實作 Phase 1 的 `.agents/skills`、`.codex/skills` 掃描、去重、override 或 Library offline 功能。
- 不改造 Board、Artifact 資料模型、Hooks、Plugins 或 Config Profiles。
- 不更改產品名稱、macOS bundle identifier、簽章、notarization 或發佈策略。
- 不為通過基準而改寫既有功能或停用失敗測試；任何上游既有失敗必須照實記錄。

## Capabilities

### New Capabilities

- `upstream-baseline`: 規範 AgentDeck 在開始功能開發前必須具備的上游來源紀錄、可重現基準驗證、依賴安全稽核與結果文件。

### Modified Capabilities

（無）

## Impact

- Affected specs: `upstream-baseline`
- Affected code:
  - Modified: `README.md`, `plan.md`
  - Conditionally modified when an audit requires remediation: `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
  - New: `BASELINE.md`
  - Removed: none
