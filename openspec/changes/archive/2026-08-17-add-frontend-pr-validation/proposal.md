## Why

Issue #4 指出一般 pull request 只在 Rust 路徑變更時觸發測試，frontend、locale 與 repository contract 的錯誤可能延後到 release workflow 才被發現。這屬於 `plan.md` Phase 7 穩定化的後續強化，需要在引入問題的 PR 上提供對應 validation gate。

## What Changes

- 擴充 pull request path coverage，使 frontend、workflow、設定與 repository check scripts 的相關變更會觸發 CI。
- 新增 Node-based PR validation job，使用鎖定依賴執行 production build、lint、locale integrity 與所有 committed repository Node contract tests。
- 保留既有 macOS、Windows Rust tests 與 Linux cargo check，不縮減跨平台覆蓋。
- 新增 repository contract checker 與 fixture tests，鎖定 PR trigger 與 frontend/repository commands，避免 workflow 與 release regression contract 漂移。

## Non-Goals

- 不重構 release、prepare-release 或發佈權限流程。
- 不在一般 PR 建置、簽署、notarize 或發布 macOS 安裝檔。
- 不新增第三方 CI action 或 JavaScript dependency。
- 不把完整 Rust matrix 複製到 Node validation job。

## Capabilities

### New Capabilities

- `pull-request-validation`: 定義 frontend 與 repository-level 變更的 PR 觸發範圍、阻擋檢查與既有 Rust 覆蓋契約。

### Modified Capabilities

(none)

## Impact

- Affected specs: `pull-request-validation`
- Affected code:
  - Modified: `.github/workflows/test.yml`, `package.json`
  - New: `scripts/check-pull-request-validation.mjs`, `scripts/check-pull-request-validation.test.mjs`
  - Removed: (none)
- External systems: GitHub Actions pull request checks
- External reference: GitHub issue #4
