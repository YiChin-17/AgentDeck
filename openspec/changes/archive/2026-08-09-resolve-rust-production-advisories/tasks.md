## 1. 固定相依路徑與修復界線

- [x] 1.1 依照「Upgrade owning dependencies instead of forcing incompatible transitive versions」保存 `quick-xml 0.38.4`、`quick-xml 0.39.4`、`rkyv 0.7.46` 的 owning dependency paths，並找出支援 `quick-xml >=0.41.0`、`rkyv >=0.8.17` 的最小 parent dependency 集合；分別以 `cargo tree --manifest-path src-tauri/Cargo.toml --target all -i quick-xml@0.38.4`、`cargo tree --manifest-path src-tauri/Cargo.toml --target all -i quick-xml@0.39.4`、`cargo tree --manifest-path src-tauri/Cargo.toml --target all -i rkyv@0.7.46` 與 manifests／lockfile diff 證明沒有強制不相容 transitive version 或無關 dependency churn。
- [x] 1.2 完成「Targeted Rust production advisories are eliminated」：只更新必要的 `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`，以及 compiler 明確要求且不改 observable behavior 的 compatibility source／tests，使 `cargo audit --file src-tauri/Cargo.lock` 對 `RUSTSEC-2026-0194`、`RUSTSEC-2026-0195`、`RUSTSEC-2026-0235` 為 0 findings，並以 `rg` 確認沒有新增 cargo-audit ignore 或 allowlist。

## 2. 保留跨平台與執行行為

- [x] 2.1 依照「Audit the full committed cross-platform lockfile」完成「Cross-platform dependency support is preserved」：執行 `cargo tree --manifest-path src-tauri/Cargo.toml --target all`，確認 resolver exit 0、vulnerable package-version pairs 已消失，且 Tauri、Wayland、logging dependency paths 仍存在；以 `git diff` 確認未移除 platform dependency 或停用 feature。
- [x] 2.2 依照「Preserve runtime behavior through verification」重跑 `cargo test --manifest-path src-tauri/Cargo.toml`、`npm ci`、`npm run build`，要求全部 exit 0 並記錄 Rust passed／failed／ignored 數與 production build 結果；若 compatibility source edit 存在，其對應 regression test 必須在同一 test run 通過。

## 3. 完成可重現的安全證據

- [x] 3.1 依照「Separate targeted vulnerabilities from allowed warnings」完成「Remediation evidence is reproducible」：在 tracked baseline 或 change 文件記錄 dependency 前後版本、執行日期、工具版本、完整命令、exit status、test counts、targeted audit 結果與未執行的 platform compile checks；以 cargo-audit 原始摘要逐項確認 allowed warnings 未被誤報為已修復或隱藏。
- [x] 3.2 執行 `spectra validate resolve-rust-production-advisories`、`spectra analyze resolve-rust-production-advisories --json`、`git diff --check` 與本 change 範圍 review，確認沒有 Critical／Warning、沒有 application behavior diff、沒有非目標 dependency 更新，並確認 working tree 只保留 manifests／lockfiles、必要 compatibility source／tests、證據與 Spectra artifacts。
