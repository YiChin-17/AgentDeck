## Why

Issue #5 指出 `build_http_client()` 會丟棄無效 proxy 或 client build 錯誤，並以未設定 proxy 的預設 client 繼續連線。這屬於 `plan.md` Phase 7 穩定化的後續安全修正；使用者明確設定 proxy 時，backend 必須 fail closed 並回報可診斷錯誤。

## What Changes

- 讓 `build_http_client()` 回傳 `Result<reqwest::blocking::Client>`，保留 proxy parse 與 HTTP client build 的錯誤脈絡。
- 讓 skills.sh 與 GitHub API callers 在發送 request 前傳播 client construction error，不直接重試未設定 proxy 的連線。
- 新增 malformed proxy、empty/no proxy 與合法 HTTP／HTTPS／SOCKS proxy 的 regression tests。
- 保留既有 upstream API 與跨平台網路行為；只移除錯誤時的 silent fallback。

## Non-Goals

- 不改變 Settings frontend 的 proxy scheme validation 或設定儲存格式。
- 不新增 proxy 自動偵測、credential 管理、retry 或 fallback proxy。
- 不修改 Git clone transport 的 proxy 實作。

## Capabilities

### New Capabilities

- `http-client-proxy-construction`: 定義共用 blocking HTTP client 對 configured、empty 與 invalid proxy 的建立及錯誤傳播契約。

### Modified Capabilities

(none)

## Impact

- Affected specs: `http-client-proxy-construction`
- Affected code:
  - Modified: `src-tauri/src/core/skillssh_api.rs`, `src-tauri/src/core/github_api.rs`
  - New: (none)
  - Removed: (none)
- External systems: skills.sh 與 GitHub HTTP APIs
- External reference: GitHub issue #5
