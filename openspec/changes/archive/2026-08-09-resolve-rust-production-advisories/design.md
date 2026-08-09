## Context

`establish-upstream-baseline` 已把可用 lockfile patch 修正的 `rustls-webpki` 與 `tar` advisories 排除，但 cargo-audit 仍對 committed lockfile 回報 5 個 vulnerabilities：`quick-xml 0.38.4` 與 `0.39.4` 各有 `RUSTSEC-2026-0194`、`RUSTSEC-2026-0195`，`rkyv 0.7.46` 有 `RUSTSEC-2026-0235`。目前 dependency paths 分別經由 Tauri／plist、Linux Wayland scanner，以及 tauri-plugin-log／byte-unit／rust_decimal 進入 lockfile。修復要求 `quick-xml >=0.41.0` 與 `rkyv >=0.8.17`，超出現有 `0.x` minor 相容界線。

## Goals / Non-Goals

**Goals:**

- 消除 committed Rust dependency graph 中上述三個 RustSec IDs 的全部 5 個 findings。
- 將 parent dependencies 更新限制在支援安全 transitive versions 的最小集合。
- 保持 Tauri 2、AgentDeck 執行行為，以及 macOS、Windows、Linux dependency targets。
- 以可重跑命令保存修復前後 dependency paths、tests 與 audit 證據。

**Non-Goals:**

- 不透過 cargo-audit ignore、allowlist、移除 platform targets 或關閉 features 取得綠色結果。
- 不處理 26 個 unmaintained、unsound 或 yanked allowed warnings。
- 不修改 UI、資料庫 schema、設定格式或使用者資料。
- 不升級與三個目標 RustSec IDs 無關的 dependencies。

## Decisions

### Upgrade owning dependencies instead of forcing incompatible transitive versions

先以 `cargo tree --target all -i` 固定三條 dependency paths，再更新直接或上層 dependencies，使其正式支援 `quick-xml >=0.41.0` 與 `rkyv >=0.8.17`。不得以手改 lockfile checksum、patch 私有 fork 或強迫違反 parent version requirement 的方式處理。替代方案是直接 pin transitive crates，但 `0.x` minor 可能有 API break，且 Cargo resolver 不會接受不相容 requirement，因此不採用。

### Audit the full committed cross-platform lockfile

cargo-audit 對 committed `src-tauri/Cargo.lock` 的完整內容驗證，`cargo tree --target all` 同時確認 macOS、Windows 與 Linux dependency paths 仍存在且可解析。不得只在目前 macOS target 隱藏其他平台 findings。替代方案是刪除非 macOS dependencies，但這違反上游跨平台相容界線。

### Separate targeted vulnerabilities from allowed warnings

完成條件針對 `RUSTSEC-2026-0194`、`RUSTSEC-2026-0195`、`RUSTSEC-2026-0235`；cargo-audit 的 allowed warnings 仍完整顯示並記錄，不在本 change 擴張處理。替代方案是一次升級所有被 warning 影響的 crates，但會混入多個無關 dependency families。

### Preserve runtime behavior through verification

依賴更新後執行 Rust tests、React／TypeScript production build 與 cross-platform dependency-tree review。若 compiler 要求 source compatibility edit，只允許不改 observable behavior 的 mechanical API adjustment，並加上對應測試。若修復必須改產品行為或離開 Tauri 2，停止實作並更新 change，不得默默擴張。

## Implementation Contract

- **Observable behavior:** AgentDeck 的 UI、資料格式、CLI 與 platform support 不變；maintainer 對 committed lockfile 執行 cargo-audit 時不再看到 `RUSTSEC-2026-0194`、`RUSTSEC-2026-0195`、`RUSTSEC-2026-0235`。
- **Dependency shape:** `cargo tree --manifest-path src-tauri/Cargo.toml --target all` 不包含 `quick-xml 0.38.4`、`quick-xml 0.39.4` 或 `rkyv 0.7.46`，且保留原本引入 Tauri、Wayland 與 logging functionality 的 dependency paths。
- **Commands:** 最終驗證至少執行 `cargo test --manifest-path src-tauri/Cargo.toml`、`cargo tree --manifest-path src-tauri/Cargo.toml --target all`、`cargo audit --file src-tauri/Cargo.lock`、`npm ci` 與 `npm run build`。
- **Failure modes:** dependency resolution、compile、test、build 或 audit 失敗時保留完整非零 exit status 與簡短錯誤證據；不得增加 ignore、allowlist、停用 tests 或刪除 platform dependencies。
- **Acceptance criteria:** 三個目標 RustSec IDs 為 0 findings；Rust tests 與 frontend build exit 0；Git diff 只包含必要 manifests／lockfiles、機械式 source compatibility edits、對應 tests 與 change artifacts；`git diff --check` exit 0。
- **Scope boundaries:** dependency remediation 與必要機械式 compatibility edits 在範圍內；產品功能、資料 migration、offline behavior、conflict handling、secret storage 與 26 個 allowed warnings 不在範圍內。無資料 migration；rollback 為還原此 change 的 dependency 與 compatibility commits。

## Risks / Trade-offs

- [Risk] 安全版本只存在於較新的 Tauri 或 platform integration release → 先找同一 major 的最小 parent upgrade，並以完整 build／tests 證明相容性。
- [Risk] `rkyv` 僅是未啟用 optional dependency，但仍留在 committed lockfile → 更新 owning dependency 使 lockfile 不再列入 vulnerable optional version，不以忽略 finding 取代 remediation。
- [Risk] Linux／Windows dependency paths 無法在 macOS 完整 compile → 使用 `cargo tree --target all` 驗證 resolver 與路徑，CI 或具備對應 toolchain 的環境負責 platform compile；未執行的 compile 不宣稱通過。
- [Risk] dependency update 帶入無關 transitive churn → review lockfile diff 並排除與目標 parent dependency paths 無關的更新。

## Migration Plan

1. 保存修復前 dependency trees 與 cargo-audit 輸出。
2. 更新最小 owning dependencies 與 lockfile，必要時加入不改行為的 compatibility edit。
3. 重跑 Rust tests、cross-platform tree、cargo-audit、frontend install 與 build。
4. review diff 後提交單一安全 remediation change；rollback 為 revert 該 commit，沒有資料 migration。

## Open Questions

無。若現有 Tauri 2 dependency ecosystem 尚無可解析的安全組合，實作必須停下並以實際 resolver 證據更新本 change。
