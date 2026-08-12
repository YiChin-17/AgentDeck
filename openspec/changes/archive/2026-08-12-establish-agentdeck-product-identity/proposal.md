## Why

AgentDeck 已從上游 Skills Manager 分化為管理 Skills、Plugins、Hooks 與 Config Profiles 的獨立產品方向，但 App bundle、視窗、選單、語系、package metadata 與圖標仍使用上游身份。若在 Phase 2 前繼續累積功能，後續改名會牽涉更多畫面、測試及持久狀態，因此應在 `protect-offline-external-library` 與 updater change 完成後建立穩定的 AgentDeck 產品身份。

## What Changes

- 將使用者可見的 App 名稱、視窗標題、macOS App menu／Tray、Settings／診斷文字、HTML title、package metadata 與主要 README 標題統一為 `AgentDeck`。
- 將主桌面 App 的 package／binary 名稱改為 `agentdeck`，但保留既有 `skills-manager-cli` 作為 Skill 專用 legacy CLI，避免把 CLI 重構混入品牌 change。
- **BREAKING**：將 Tauri Bundle ID 從 `com.agentskills.desktop` 改為穩定的 `io.github.yichin17.agentdeck`；macOS 會將新 bundle 視為不同 App，實作時必須驗證核心 Library／SQLite／設定仍由既有明確路徑解析，不依賴 Bundle ID。
- 以 AgentDeck-owned 原始圖稿取代上游 App icon，重新產生 macOS、Windows 與通用 desktop icon assets；macOS Tray 使用可適應深色／淺色選單列的單色 template icon。
- 保留 `.skills-manager`、`skills-manager.db`、backup metadata、Git refs／trailers、Keychain service、localStorage keys 等既有持久協議名稱，將它們明確標示為 legacy compatibility identifiers，不做無依據的全面字串取代。
- 新增可重複執行的產品身份檢查，驗證指定的使用者可見與 bundle surfaces 使用 AgentDeck，並允許 attribution、legacy protocol、CLI 及資料相容位置保留 Skills Manager 名稱。
- 更新 `plan.md`，記錄 AgentDeck 的穩定 display name、Bundle ID、圖標來源與 legacy identifier 邊界。

## Non-Goals

- 不移除上游 attribution、MIT License、`BASELINE.md` 或 Git upstream。
- 不重新命名 `.skills-manager`、`skills-manager.db`、`.skills-manager` backup protocol、`refs/skills-manager/*`、Git trailers、Keychain service 或現有 localStorage keys。
- 不重新命名或擴充 `skills-manager-cli`；Artifact-wide CLI 由後續 Phase 3+ change 設計。
- 不改 GitHub OAuth App client ID、OAuth 授權頁的實際第三方名稱或 backup remote protocol；若要擁有獨立 OAuth 身份須另開 change。
- 不移除 App updater；該工作由已 parked 的 `remove-upstream-app-updater` 先行處理。
- 不變更 Library availability、SQLite schema、Artifact 模型或外部 Library 路徑。
- 不建立公開發佈、signing、notarization 或 auto-update 管線。

## Capabilities

### New Capabilities

- `product-identity`: 定義 AgentDeck 的 display name、Bundle ID、桌面圖標與使用者可見品牌，同時保留必要的 legacy compatibility identifiers 與 upstream attribution。

### Modified Capabilities

(none)

## Impact

- Affected plan: Phase 0 的 fork 身份後續整理，安排在 Phase 1 完成與 `remove-upstream-app-updater` 實作後、Phase 2 新功能開始前；不納入 `protect-offline-external-library`。
- Intentional upstream divergence: App bundle、display name、圖標與產品文案改為 AgentDeck；upstream code provenance、legacy storage／backup protocol 與 Skill CLI 相容性保留。
- Affected specs: `product-identity`
- Affected code and assets:
  - Modified: `src-tauri/tauri.conf.json`
  - Modified: `src-tauri/Cargo.toml`
  - Modified: `src-tauri/Cargo.lock`
  - Modified: `package.json`
  - Modified: `package-lock.json`
  - Modified: `index.html`
  - Modified: `src-tauri/src/lib.rs`
  - Modified: `src-tauri/src/commands/settings.rs`
  - Modified: `src/views/Settings.tsx`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - Modified: `README.md`
  - Modified: `plan.md`
  - Modified: `assets/icon.png`
  - Modified: `src-tauri/icons/icon-source.png`
  - Modified: `src-tauri/icons/icon.png`
  - Modified: `src-tauri/icons/32x32.png`
  - Modified: `src-tauri/icons/64x64.png`
  - Modified: `src-tauri/icons/128x128.png`
  - Modified: `src-tauri/icons/128x128@2x.png`
  - Modified: `src-tauri/icons/icon.icns`
  - Modified: `src-tauri/icons/icon.ico`
  - Modified: `src-tauri/icons/Square30x30Logo.png`
  - Modified: `src-tauri/icons/Square44x44Logo.png`
  - Modified: `src-tauri/icons/Square71x71Logo.png`
  - Modified: `src-tauri/icons/Square89x89Logo.png`
  - Modified: `src-tauri/icons/Square107x107Logo.png`
  - Modified: `src-tauri/icons/Square142x142Logo.png`
  - Modified: `src-tauri/icons/Square150x150Logo.png`
  - Modified: `src-tauri/icons/Square284x284Logo.png`
  - Modified: `src-tauri/icons/Square310x310Logo.png`
  - Modified: `src-tauri/icons/StoreLogo.png`
  - Modified: `src-tauri/icons/tray/tray-icon-16.png`
  - Modified: `src-tauri/icons/tray/tray-icon-20.png`
  - Modified: `src-tauri/icons/tray/tray-icon-24.png`
  - Modified: `src-tauri/icons/tray/tray-icon-32.png`
  - New: `scripts/check-product-identity.mjs`
- Affected checks: React／TypeScript production build、ESLint、Rust tests、i18n integrity、Tauri bundle metadata inspection、icon render inspection 與新的 product identity check。
- Migration and rollback: core Library／SQLite／backup／Keychain paths不改；Bundle ID 切換後舊 App bundle 需人工移除，rollback 需還原 display name、Bundle ID 與 icon assets，但不得刪除使用者資料。
- Dependencies and secrets: 不新增 runtime dependency，不新增或移動 secrets。
