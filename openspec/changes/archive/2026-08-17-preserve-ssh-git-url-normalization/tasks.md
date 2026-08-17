## 1. Regression contract

- [x] 1.1 在 `src-tauri/src/core/git_fetcher.rs` 新增先失敗的 `ssh://git@github.com/acme/skills.git` parsing test，交付「Supported complete Git URLs retain their transport form」並以該 test 確認 clone URL、branch 與 subpath 的精確輸出。
- [x] 1.2 補齊 HTTP、HTTPS、`git@`、shorthand 與 GitHub tree cases，交付「Existing shorthand and GitHub tree normalization remains stable」，並以 `cargo test --locked --manifest-path src-tauri/Cargo.toml core::git_fetcher::tests::parse` 確認既有輸出不變。

## 2. Normalization fix

- [x] 2.1 讓 `normalize_url` 將 `ssh://` 納入完整 URL passthrough，確保合法 SSH URL 不進入 shorthand branch，並以 1.1 與 1.2 的 regression tests 驗證。
- [x] 2.2 執行 `cargo test --locked --manifest-path src-tauri/Cargo.toml`，確認完整 Rust suite 通過且本修正沒有改變其他 Git、同步或跨平台行為。
