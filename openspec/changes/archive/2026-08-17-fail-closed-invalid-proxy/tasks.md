## 1. HTTP client construction contract

- [x] 1.1 依「Factory unit tests 驗證 proxy 邊界」在 `src-tauri/src/core/skillssh_api.rs` 新增先失敗的 malformed、None、empty、HTTP、HTTPS 與 SOCKS5 cases，交付「Empty and absent proxy values preserve direct client behavior」與「Supported configured proxy schemes remain usable」，並以 `cargo test --locked --manifest-path src-tauri/Cargo.toml core::skillssh_api` 驗證且不發出 network request。
- [x] 1.2 依「HTTP client factory 回傳 anyhow Result」將 `build_http_client` 改為 `Result<reqwest::blocking::Client>`，以不含 proxy 原值的固定 `Context` 傳播 parse/build errors，交付「Configured proxy failures stop HTTP client construction」，並以 1.1 tests 驗證 malformed proxy 回 Err 且沒有 default fallback。

## 2. Caller propagation

- [x] 2.1 依「所有 HTTP callers 在送出 request 前傳播 construction error」更新 `fetch_leaderboard`、`search_skills`、`connect_backup_repo`、`device_flow_start` 與 `device_flow_poll` 使用 `?`，確保 client construction 失敗不會送出 request 或改走 direct client，並以 `cargo check --locked --manifest-path src-tauri/Cargo.toml` 驗證所有 callers 符合新 signature。
- [x] 2.2 執行 `cargo test --locked --manifest-path src-tauri/Cargo.toml`，確認完整 Rust suite 通過，且 skills.sh／GitHub API response handling 與 Git transport proxy 行為未被改動。
