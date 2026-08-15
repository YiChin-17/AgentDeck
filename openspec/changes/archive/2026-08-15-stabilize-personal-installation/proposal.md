## Summary

完成 Phase 7 的個人安裝穩定化：以鎖定依賴 regression、既有資料 migration smoke、temporary Codex／Claude Projects 與實際 macOS bundle 驗證，證明本機產出的 AgentDeck 可沿用既有資料安全啟動與使用，並提供可執行的安裝、備份及解除安裝說明。

## Motivation

Phase 0–6 的功能與安全 contract 已完成，但目前只有開發環境測試，尚未把 packaged AgentDeck 的 build artifact、首次啟動資料相容性、Library Online／Offline、主要 workflow smoke 與個人安裝文件串成一次可重複驗收。若直接把 development build 當作可安裝結果，使用者無法確認 bundle identity、既有 `.skills-manager` 資料、backup／Keychain contract、解除安裝邊界與缺少 auto-update 的政策是否在實際 `.app` 中仍成立。

## Proposed Solution

- 建立 repository-owned 個人安裝檢查，驗證鎖定依賴 suites、production audits、macOS `.app`／installer artifact、Bundle ID／版本、禁止 updater 的 build surface與必要文件內容。
- 建立 packaged-app smoke checklist 與可追溯證據，覆蓋既有 Library／SQLite migration、internal／external Library Online／Offline、Skills sync／conflict、CLI contract、Hook／Plugin／Config Profile preview-first主要流程；若任何既有 contract 失敗，先以 failing regression test具體化後再做最小修正。
- 更新個人安裝文件，說明本機 build、安裝／首次啟動、既有 Skills Manager 資料沿用、Library位置與 offline、Git backup／restore、解除安裝及資料保留。
- 延伸 app update policy，要求個人安裝文件明確揭露目前沒有 application auto-update、公開 distribution、Developer ID signing、notarization或 release hosting保證。

## Non-Goals

- 不建立公開 release、對外 distribution、release hosting、GitHub Releases upload、Developer ID signing、notarization或 App Store流程。
- 不加入 update endpoint、update manifest、update public key、Tauri updater dependency／permission、binary release check或 application auto-update UI。
- 不新增 Skill、Plugin、Hook、Config Profile、Library、Git backup或 CLI管理能力；只有既有 contract 出現可重現 regression時才修正。
- 不重新命名或搬移 `.skills-manager`、`skills-manager.db`、Git backup metadata／refs／trailers、`skills-manager-git-backup` Keychain service、既有 localStorage keys或 `skills-manager-cli`。
- 不把 generated `.app`、`.dmg`、build cache、machine-specific absolute path、token、credential或使用者資料提交進 Git。
- 不宣稱 Windows／Linux installer 已完成；保留既有跨平台程式碼與 build configuration，不為 macOS smoke破壞上游相容性。

## Alternatives Considered

- 直接把 npm run tauri:build 成功視為完成：無法證明 packaged app可啟動、資料沿用、offline與主要 workflow，因此拒絕。
- 在此階段一併加入 signing、notarization與 auto-update：會引入外部帳號、發佈信任鏈與 runtime updater，超出個人安裝需求，因此保留給獨立 Spectra change。
- 只寫人工 checklist、不建立 repository check：無法防止之後重新加入 updater surface或遺漏必要文件，因此採自動 contract加人工 packaged smoke。

## Capabilities

### New Capabilities

- `personal-installation-readiness`: 鎖定依賴驗收、packaged AgentDeck artifact／啟動 smoke、既有資料與主要 workflow相容證據，以及個人安裝／備份／解除安裝文件 contract。

### Modified Capabilities

- `app-update-policy`: 將 personal-installation policy從計畫層延伸到使用者文件與 packaged build驗收，明確禁止文件或 artifact暗示現階段具備 application auto-update或公開 distribution trust。

## Impact

- Affected phase: `plan.md` Phase 7 穩定化與個人安裝。
- Affected specs: `personal-installation-readiness`、`app-update-policy`。
- Affected code and documents:
  - New: `scripts/check-personal-installation.mjs`、`scripts/check-personal-installation.test.mjs`、`scripts/frontend-argument-surface.mjs`、`scripts/check-ui-command-arguments.test.mjs`、`docs/personal-installation-verification.md`。
  - Modified: `README.md`、`package.json`、`scripts/check-no-upstream-app-updater.test.mjs`、`.gitignore`、`plan.md`。
  - Located regression fixes（Phase 7 驗收實際定位，已附失敗輸出）：`src-tauri/src/core/config_profile_inventory.rs`、`scripts/check-hooks-ui.mjs`、`scripts/check-plugins-ui.mjs`。
  - Conditional regression fixes: 除上列已定位項目外，仍僅在既有 test或packaged smoke以具體失敗證據定位後，先更新本 change artifacts列出確切 project-relative path，再依 TDD修正；proposal不預先授權未定位的 runtime改動。
  - Removed: none.
- Build outputs: 驗證 `src-tauri/target/release/bundle/macos/AgentDeck.app` 與當次 Tauri實際產出的 macOS installer，但不追蹤 generated bundle。
- Dependencies: 沿用 Node.js／Rust standard tooling、既有 npm／Cargo dependencies、Tauri CLI與 macOS內建工具，不新增 production dependency。
- Compatibility: 保留既有 macOS-first與上游跨平台邊界、固定 Bundle ID `io.github.yichin17.agentdeck`、Library／SQLite／backup／Keychain／localStorage／CLI持久 contract。
