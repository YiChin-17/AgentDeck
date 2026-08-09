## 1. 固定上游來源與文件邊界

- [x] 1.1 依照「Preserve the fork point as immutable evidence」完成「Upstream provenance is recorded」：在 `BASELINE.md` 記錄 `origin`、`upstream`、完整起點 commit `ab2a6947062c49640b751d4c2a9d8be816347dc1`、tag `v1.30.0` 與 MIT License，並以 `git remote -v`、`git rev-parse upstream/main`、`git describe --tags --exact-match ab2a694` 及人工比對 `LICENSE` 驗證。
- [x] 1.2 依照「Store reproducible evidence in tracked documents」完成「Project direction and upstream compatibility are explicit」：讓 `README.md` 可直接辨識上游 attribution、AgentDeck 管理範圍、macOS-first 目標及保留跨平台能力的界線，並由人工逐項比對 spec 四項資訊及確認 `plan.md` Phase 0 狀態只引用本次實際結果。

## 2. 建立未修改依賴的建置與測試基準

- [x] 2.1 依照「Use locked dependencies and repository-owned commands」完成「Baseline verification uses locked dependencies」的前端部分：記錄 `node --version` 與 `npm --version`，執行 `npm ci`、`npm run build`，把各指令 exit status 與 production build 結果寫入 `BASELINE.md`，並以 `git diff -- package.json package-lock.json` 無非預期差異驗證鎖定依賴未先被改寫。
- [x] 2.2 依照「Use locked dependencies and repository-owned commands」完成 Rust 基準：記錄 `rustc --version` 與 `cargo --version`，執行 `cargo test --manifest-path src-tauri/Cargo.toml`，把 exit status、passed／failed／ignored 數量寫入 `BASELINE.md`，並以 `git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock` 無非預期差異驗證原始 Rust dependency graph。

## 3. 稽核 production dependencies 並限制修復範圍

- [x] 3.1 依照「Separate baseline observation from advisory remediation」完成「Production dependency advisories are evaluated safely」：在尚未修改 manifests／lockfiles 前執行 `npm audit --omit=dev` 與 `cargo audit --file src-tauri/Cargo.lock`，將工具版本、exit status、production advisory 數量與受影響套件寫入 `BASELINE.md`，並以命令輸出和文件逐項比對驗證未隱藏失敗。
- [x] 3.2 若 3.1 發現可相容修復的 production advisory，只更新受影響的 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml` 或 `src-tauri/Cargo.lock`，重跑對應的 install、build、tests 與 audit 並記錄前後結果；以最終 audit 無該 advisory、所有受影響驗證成功及 `git diff` 不含應用行為修改作為完成條件。若需要 breaking upgrade 或產品行為修改，則在 `BASELINE.md` 記錄未解項並另建 change，本 task 不修改依賴。

## 4. 完成可重現且不洩漏本機資料的基準

- [x] 4.1 完成「Verification evidence is reproducible」：確認 `BASELINE.md` 對每個必要指令都有執行日期、工具版本、完整命令、exit status、結果摘要及測試數量，並由另一輪逐項內容審查確認所有「通過」敘述都有本次命令輸出支持。
- [x] 4.2 完成「Baseline artifacts exclude local and sensitive data」：依照「Store reproducible evidence in tracked documents」檢查本 change 新增或修改的內容只使用 project-relative paths，且不含 token、登入資訊、套件快取、generated build output、Spectra local database 或新增的機器專屬絕對路徑；以 `git status --short`、`git diff --check`、`git diff --stat` 及針對本次 diff 的敏感字串人工審查驗證。
- [x] 4.3 執行 `spectra validate establish-upstream-baseline` 與 `spectra analyze establish-upstream-baseline --json`，確認沒有 Critical／Warning，並檢查 Git working tree 僅保留本 change 的基準文件與有 audit 證據支持的相容依賴修正；驗證後在 `plan.md` 將 Phase 0 結果標記為完成或如實保留未完成原因。
