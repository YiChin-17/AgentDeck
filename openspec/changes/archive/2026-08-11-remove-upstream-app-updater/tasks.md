## 1. 完整移除 App updater，而不是只改 endpoint

- [x] 1.1 先為「AgentDeck does not check for application binary releases」新增會失敗的 repository source assertion，再移除 `check_app_update`、`AppUpdateInfo`、frontend IPC wrapper、`AppContext` 啟動檢查與通知，使 App 啟動及 Settings 不再查詢 release；以 assertion、`npm run build` 與搜尋 invoke registrations 確認 command 和 caller 均不存在。
- [x] 1.2 為「AgentDeck cannot download or install application updates」新增 Settings UI source assertion，再移除 `checkUpdater`、下載安裝／重啟套用 handler、相關 state 與 en／zh-TW updater 文案，使所有 Settings actions 都無法安裝 App binary；以 assertion、`npm run check:i18n` 及人工檢視 Settings 確認控制項和 toast 均不存在。
- [x] 1.3 完成同一 requirement 的 Tauri build surface：移除 updater plugin registration、`updater:default`、endpoint、pubkey、`createUpdaterArtifacts` 與 JavaScript／Rust dependencies，使 packaged App 不具 updater 權限或產物；以 manifests 搜尋、`npm install --package-lock-only --ignore-scripts` 後的 lockfile diff、`cargo check --manifest-path src-tauri/Cargo.toml --locked` 驗證依賴與設定不可達。

## 2. 上游同步保留在開發流程

- [x] 2.1 依「Upstream provenance remains separate from runtime update trust」檢查 `README.md`、`BASELINE.md`、License 與 Git remote 說明仍保留 upstream provenance，同時 runtime 不再將 upstream release 當更新來源；以文件內容 review、`git remote -v` 與 updater source check 驗證 attribution 保留且 binary trust 已移除。

## 3. 用限定範圍的靜態檢查防止更新路徑回歸

- [x] 3.1 先為「Repository checks prevent application updater regression」建立 fixture／自我測試案例，再實作 `scripts/check-no-upstream-app-updater.mjs` 並加入 `package.json` script，使 runtime／build surfaces 出現 release query、plugin、permission、endpoint、pubkey 或 dependency 時非零結束且列出命中位置；以乾淨案例通過、每類違規 fixture 失敗、attribution fixture 通過驗證範圍。

## 4. 個人安裝策略取代公開發佈承諾

- [x] 4.1 依「Personal installation is the documented release policy」更新 `plan.md` Phase 7，保留 stabilization、本機 build、備份與解除安裝驗證，並把公開 release、signing distribution、notarization 與 auto-update 延後到新 change；以 Phase 7 內容 review 及 `spectra validate remove-upstream-app-updater` 驗證範圍與 proposal 一致。

## 5. 整體驗收

- [x] 5.1 執行 `npm run build`、`npm run lint`、`npm run check:i18n`、新的 updater source check 與 `cargo test --manifest-path src-tauri/Cargo.toml`，記錄各命令 exit status 及 Rust test pass／fail count，確認移除 updater 未破壞既有 App build、語系或 backend behavior。
- [x] 5.2 啟動 App，等待超過原本三秒 release check 時段並打開 Settings，確認沒有 App 更新網路請求、通知、檢查或安裝控制項；同時確認 Skill update 與 Git upstream 開發文件仍存在，保存人工驗證結果。
  - 驗證記錄（2026-08-11）：`npx tauri build --debug --bundles app` exit 0；啟動產出的 `.app` 後等待 4 秒，通知區為空，`nettop` 無連線列，App log 對 release updater 關鍵字零命中。原生 Settings 保留「Skill 自動更新」與「Git 同步設定」，About 區只保留說明、回報問題、匯出日誌與 GitHub，沒有 App 更新檢查、下載、安裝、重啟控制項或新版通知。README、BASELINE、MIT License 與 `upstream` remote 已於 2.1 驗證保留。
