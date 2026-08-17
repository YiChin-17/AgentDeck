## 1. Pull-request workflow contract

- [x] 1.1 依「Repository checker 鎖定 PR workflow contract」在 `scripts/check-pull-request-validation.test.mjs` 建立先失敗的 fixtures，交付「Repository contract detects pull-request workflow drift」對 restrictive trigger、缺少 Node command 與遺失 Rust coverage 的精確 findings，並以 `node --test scripts/check-pull-request-validation.test.mjs` 驗證。
- [x] 1.2 實作零依賴 `scripts/check-pull-request-validation.mjs` 並在 `package.json` 暴露 `check:pull-request-validation`，使合格 workflow exit 0、違規 fixture exit non-zero 且指出 file/rule，並以 1.1 tests 與 `npm run check:pull-request-validation` 驗證。

## 2. GitHub Actions validation

- [x] 2.1 依「Pull requests 不使用 top-level path filter」移除 `.github/workflows/test.yml` 的 restrictive `pull_request.paths`，交付「Pull requests trigger repository validation without a restrictive path filter」，並以 `npm run check:pull-request-validation` 驗證 frontend、scripts、package 與 workflow 變更不會被 trigger 排除。
- [x] 2.2 依「Node validation job 對齊 release regression commands」新增 Node 22 Ubuntu job，依序執行 `npm ci`、`npm run build`、`npm run lint`、`npm run check:i18n` 與 `node --test scripts/*.test.mjs`，交付「Pull-request Node validation uses locked repeatable gates」，並以 checker fixtures 及 workflow source assertion 驗證每個 command 都是 blocking step。
- [x] 2.3 保留現有 macOS／Windows Rust test matrix 與 Linux cargo check，交付「Existing cross-platform Rust pull-request coverage remains available」，並以 `npm run check:pull-request-validation` 驗證三個 platform contracts 仍存在。

## 3. Repository verification

- [x] 3.1 執行 `node --test scripts/*.test.mjs`、`npm run build`、`npm run lint` 與 `npm run check:i18n`，確認新 checker、完整 Node contracts 與 PR job 使用的本地 commands 全部通過。
