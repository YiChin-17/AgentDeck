## Context

Settings 內同一個 `GITHUB_URL` 同時供 repository button 與 bug-report flow 使用，目前值仍是 upstream repo。產品識別檢查已能掃描受管 surfaces 並回報穩定 rule，但尚未把 AgentDeck-owned repository destination 納入契約。

## Goals / Non-Goals

**Goals:**

- 讓兩個 Settings actions 都導向 `YiChin-17/AgentDeck`。
- 讓 product identity checker 以明確 rule 阻止這兩個入口回到 upstream URL。
- 保留 upstream attribution 與 legacy compatibility allowlist。

**Non-Goals:**

- 不集中管理全專案的所有 URL。
- 不變更 issue template、diagnostic clipboard payload 或其他 Settings 功能。
- 不禁止 upstream URL 出現在 README、baseline provenance 或其他明確 attribution surface。

## Decisions

### Settings actions 共用固定 AgentDeck repository constant

保留目前兩個 actions 共用單一 constant 的結構，只把值改為 `https://github.com/YiChin-17/AgentDeck`。相較引入 runtime setting 或環境變數，固定值符合這個 fork 的產品識別契約，也避免使用者可設定內容影響官方回報目的地。

### Product identity checker 驗證受管 Settings destinations

在既有 `scripts/check-product-identity.mjs` 增加專門的 repository destination rule，驗證 Settings source 含正確 base URL，且兩個 actions 分別使用 base 與其 `/issues/new?template=bug_report.md` 衍生 URL。測試 fixture 同時涵蓋正確值與 upstream regression。相較全 repository 禁止 upstream URL，受管 surface 檢查不會破壞合法 attribution。

## Implementation Contract

- Behavior：使用者按 Settings 的 GitHub button 時，系統開啟 `https://github.com/YiChin-17/AgentDeck`；執行回報問題流程時，既有 diagnostics copy 行為完成後開啟同一 repo 下的 bug report template URL。
- Interface：不改變 React component props、Tauri IPC 或 diagnostics payload；只調整 Settings 內部 repository constant 與 product identity checker 的 findings。
- Failure modes：`openUrl` 失敗仍沿用既有 toast／logging；checker 發現缺少正確 URL、回到 upstream URL 或 actions 未使用受管 constant 時，必須以非零狀態回報受影響檔案與穩定 rule 名稱。
- Acceptance：`node --test scripts/check-product-identity.test.mjs` 必須覆蓋正確 fixture 與 upstream regression；`npm run check:product-identity`、`npm run build` 與 `npm run lint` 必須通過。
- In scope：`src/views/Settings.tsx` 的兩個 repository destinations 與現有 product identity checker/test。
- Out of scope：其他外部連結、issue template 內容、upstream attribution、legacy identifiers 與 UI layout。

## Risks / Trade-offs

- [Risk] 全域 upstream URL 禁令會誤傷合法 attribution → 只檢查 Settings 受管 destination 與 action wiring。
- [Risk] 只檢查字串存在仍可能讓 action 使用另一個 URL → fixture tests 必須驗證兩個 action 都從受管 constant 建構目的地。
