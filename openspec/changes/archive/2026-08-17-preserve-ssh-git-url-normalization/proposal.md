## Why

Issue #2 指出 Git 來源驗證接受 `ssh://`，但正規化階段未把它視為完整 URL，導致合法來源被改寫成無效的 GitHub HTTPS shorthand。這屬於 `plan.md` Phase 7 穩定化的後續修正，必須讓驗證與正規化採用一致的協議集合。

## What Changes

- 將合法的 `ssh://` Git URL 原樣傳遞至 clone layer，不再套用 shorthand 改寫。
- 新增正規化 regression test，並確認既有 HTTP、HTTPS、`git@`、GitHub tree URL 與 shorthand 行為不變。
- 保留上游跨平台 Git 行為；本修正只補齊上游已宣告支援但未正規化的 URL 形式。

## Non-Goals

- 不新增其他 Git URL scheme。
- 不變更 SSH credential、host key 或 clone transport 處理。
- 不改寫既有 HTTP、HTTPS、`git@`、GitHub tree URL 或 shorthand 規則。

## Capabilities

### New Capabilities

- `git-source-url-normalization`: 定義通過驗證的完整 Git URL 與 shorthand 在正規化後必須保留或改寫的契約。

### Modified Capabilities

(none)

## Impact

- Affected specs: `git-source-url-normalization`
- Affected code:
  - Modified: `src-tauri/src/core/git_fetcher.rs`
  - New: (none)
  - Removed: (none)
- External reference: GitHub issue #2
