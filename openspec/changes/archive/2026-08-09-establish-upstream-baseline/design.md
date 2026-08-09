## Context

AgentDeck 以 `xingkongliang/skills-manager` v1.30.0 的 commit `ab2a694` 為 fork 起點。本機已有 `origin` 與 `upstream`，但尚未在目前環境重新安裝依賴、驗證前端與 Rust 基準，也尚未把本次結果與產品分歧方向寫入可追蹤文件。這個 change 跨越專案文件、Node.js 前端工具鏈、Rust workspace 與兩套依賴安全稽核，因此需要固定執行順序與修復界線。

## Goals / Non-Goals

**Goals:**

- 建立能由後續維護者重跑的 Phase 0 基準流程。
- 保存上游來源、實際起點、指令、環境版本、測試數量與 audit 結果。
- 區分上游原始狀態與 AgentDeck 為安全 advisory 所做的最小修正。
- README 清楚說明 MIT 上游來源、AgentDeck 方向與跨平台相容原則。

**Non-Goals:**

- 不變更應用程式執行行為、資料模型或使用者介面。
- 不開始 Phase 1 之後的功能。
- 不處理只存在於 development dependency 且不影響 production artifact 的 advisory。
- 不建立發佈、簽章、notarization 或自動更新流程。

## Decisions

### Preserve the fork point as immutable evidence

以 `ab2a694`／`v1.30.0` 作為不可改寫的上游基準，並在 `BASELINE.md` 記錄 `origin`、`upstream` 和驗證日期。替代方案是只依賴 Git remote 與歷史，但 remote 可被改名，且無法保存某次驗證所針對的確切版本，因此不採用。

### Use locked dependencies and repository-owned commands

前端先使用 `npm ci` 按 `package-lock.json` 安裝，再執行 `npm run build`；Rust 使用已提交的 `src-tauri/Cargo.lock` 執行 workspace tests。替代方案是重新解析最新版依賴，但這會讓基準偏離 fork 起點，無法判定差異來自上游或依賴漂移。

### Separate baseline observation from advisory remediation

先在未修改 manifest／lockfile 的狀態完成 build、tests 與 production audit 並記錄結果。只有 audit 證明 production dependency 有 advisory，且可用相容版本修復時，才修改對應 manifest／lockfile；修改後必須重跑受影響的 build、tests 與 audit。不得為了得到綠色結果停用測試、忽略 audit exit code 或改動產品行為。

替代方案是看到 advisory 後立即升級所有依賴，但這會混入無關版本更新並破壞基準可比性，因此不採用。

### Store reproducible evidence in tracked documents

`BASELINE.md` 保存環境版本、指令、exit status、測試通過／失敗數與 advisory 摘要；`plan.md` 只更新 Phase 0 的高階狀態與最新結論，`README.md` 保存面向使用者的來源與產品方向。不得提交 build 產物、套件快取、完整機器路徑、token 或登入資訊。

替代方案是只在對話或 commit 訊息保存結果，但它們不構成穩定且可更新的專案基準，因此不採用。

## Implementation Contract

- **Observable result:** repository readers can identify the upstream repository, fork point, AgentDeck direction, exact baseline commands, execution date, tool versions, pass/fail counts, and unresolved production advisories without relying on this conversation.
- **Commands:** frontend baseline uses `npm ci` and `npm run build`; Rust baseline uses the workspace test command applicable to `src-tauri/Cargo.toml`; JavaScript and Rust production dependency audits use the package managers already represented by committed lockfiles.
- **Failure modes:** any failed install, build, test, or audit is recorded with its exit status and concise error evidence. A failure does not authorize disabling checks or expanding into application fixes; unresolved failures keep the Phase 0 completion status open.
- **Acceptance criteria:** `BASELINE.md` contains all required evidence, README contains upstream attribution and AgentDeck direction, the license remains MIT, every claimed passing command has been executed in the current checkout, and `git diff --check` reports no formatting errors.
- **Scope boundaries:** documentation and minimal production advisory remediation are in scope. Feature work, UI changes, data migrations, runtime offline behavior, configuration conflict handling, and secret storage are out of scope. No application data migration or rollback is required because this change does not alter runtime data; rollback is reverting the Phase 0 commit.

## Risks / Trade-offs

- [Risk] Native Rust or Tauri prerequisites may be missing on the current machine → Record the missing prerequisite and exact failure; do not claim the baseline passed.
- [Risk] Security remediation may require a breaking dependency upgrade → Leave the advisory documented and create a separate proposal instead of expanding this change.
- [Risk] Audit databases change over time, so future results can differ at the same commit → Record execution date and tool versions alongside results.
- [Risk] Machine-specific paths or credentials can leak into evidence → Keep only project-relative paths and redact tokens, account data, and user-specific environment values.

## Migration Plan

1. Capture the unmodified upstream baseline and document its evidence.
2. Apply only compatible production advisory remediation when required, then repeat affected verification.
3. Review the final diff for Phase 0-only scope and create one dedicated commit.
4. Roll back by reverting that commit; no database or user-data migration is involved.

## Open Questions

無。若 audit 只能透過 breaking upgrade 修復，該升級必須另建 change 決定。
