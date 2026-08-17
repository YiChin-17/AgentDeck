## Why

Issue #3 指出 AgentDeck 的 Settings 仍把 GitHub 與回報問題入口導向上游 Skills Manager，會讓使用者辨識錯誤並把診斷資料送到錯誤 repo。這屬於 `plan.md` Phase 0 fork 與產品識別的後續修正，AgentDeck 擁有的介面必須指向 fork 本身。

## What Changes

- 將 Settings 的 GitHub repository 入口改為 `https://github.com/YiChin-17/AgentDeck`。
- 將 Settings 的 bug report 入口改為 AgentDeck repo 的 issue 建立頁。
- 擴充 product identity 靜態檢查與測試，阻止上游 repo URL 再次出現在這兩個 AgentDeck-owned surfaces。
- 這是 AgentDeck 相對上游的刻意產品識別差異，不改變記錄 upstream provenance 的文件與相容性位置。

## Non-Goals

- 不移除合法的 upstream attribution、remote 記錄或歷史相容性字串。
- 不變更 GitHub issue template 的內容或診斷資料格式。
- 不重新設計 Settings 畫面或其他外部連結。

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `product-identity`: AgentDeck-owned Settings repository 與回報問題入口必須指向 `YiChin-17/AgentDeck`，並由靜態契約防止回歸。

## Impact

- Affected specs: `product-identity`
- Affected code:
  - Modified: `src/views/Settings.tsx`, `scripts/check-product-identity.mjs`, `scripts/check-product-identity.test.mjs`
  - New: (none)
  - Removed: (none)
- External reference: GitHub issue #3
