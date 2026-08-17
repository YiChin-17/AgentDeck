## Context

`build_http_client(proxy_url, timeout_secs)` 是 skills.sh 與 GitHub API 共用的 blocking HTTP client factory。目前 proxy parse error 被 `if let Ok` 丟棄，client build error 被 `unwrap_or_default()` 轉成預設 client，兩者都可能在使用者明確設定 proxy 後改走 direct connection。

## Goals / Non-Goals

**Goals:**

- 讓 proxy parse 與 client build errors 沿既有 `anyhow::Result` 呼叫鏈傳回 commands／CLI。
- 保證 configured proxy 建立失敗時，在任何 request send 前停止。
- 保留 None、empty 與合法 HTTP／HTTPS／SOCKS proxy 的既有行為。

**Non-Goals:**

- 不修改 frontend proxy validation、setting storage 或 Git transport proxy。
- 不新增 retry、direct fallback、proxy auto-detection 或 credential handling。
- 不更改 HTTP API response parsing 與既有 caller-facing error mapping。

## Decisions

### HTTP client factory 回傳 anyhow Result

將 signature 改為 `build_http_client(proxy_url: Option<&str>, timeout_secs: u64) -> Result<reqwest::blocking::Client>`。`reqwest::Proxy::all` 與 `ClientBuilder::build` 都以 `Context` 加上不含 credential 的固定階段訊息後用 `?` 傳播。相較自訂 error enum，現有 callers 已使用 `anyhow::Result`，不需要新增型別或 dependency。

### 所有 HTTP callers 在送出 request 前傳播 construction error

`fetch_leaderboard`、`search_skills`、`connect_backup_repo`、`device_flow_start` 與 `device_flow_poll` 取得 client 時使用 `?`。不在 caller 端捕捉後重建 client，確保 configured proxy 失敗時沒有 direct fallback。

### Factory unit tests 驗證 proxy 邊界

在 `skillssh_api` 同檔測試直接呼叫 factory：malformed non-empty proxy 必須回 Err；None 與 empty string 必須回 Ok；合法 HTTP、HTTPS 與 SOCKS5 URL 必須回 Ok。測試只驗證 client construction，不發出外部 network request。

## Implementation Contract

- Behavior：configured malformed proxy 或 client build failure 會在 request 送出前回傳 error；系統不得以未設定 proxy 的 client 重試。None 或 empty proxy 建立一般 direct client；合法 proxy 建立設定該 proxy 的 client。
- Interface：`build_http_client` 的回傳型別由 Client 改為 `Result<Client>`；五個既有 callers 維持原本 public return shapes，透過 `?` 將 construction error 納入既有 `Result`。
- Failure modes：proxy parse error 包含固定 proxy configuration context，但不得包含可能帶 credential 的完整 proxy URL；client build error 包含固定 client construction context；兩者都不得 fallback。
- Acceptance：`cargo test --locked --manifest-path src-tauri/Cargo.toml core::skillssh_api` 的 malformed、None、empty、HTTP、HTTPS 與 SOCKS5 cases 通過，完整 `cargo test --locked --manifest-path src-tauri/Cargo.toml` 亦通過。
- In scope：共用 blocking HTTP factory、五個 skills.sh／GitHub callers 與 factory unit tests。
- Out of scope：Git clone/fetch proxy、frontend setting validation、credential storage、network retry 與 response errors。

## Risks / Trade-offs

- [Risk] 改變 factory signature 造成 caller 編譯失敗 → 以 `rg` 列出所有 callers，逐一改為 `?`，再以完整 Rust tests 驗證。
- [Risk] error context 洩漏含帳密的 proxy URL → context 使用固定訊息，不格式化原始 proxy 字串。
- [Risk] `reqwest` 對部分字串採寬鬆解析 → regression test 使用能穩定觸發 parse failure 的明確 malformed scheme syntax。
