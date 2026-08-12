## Context

目前 repository 文件已描述 AgentDeck，但 build metadata 與執行中的 App 仍沿用上游身份：Tauri `productName`、window title、Bundle ID、Cargo／npm package、HTML title、Rust tray/menu 文字、i18n 文案及 icon assets 都可見 `Skills Manager` 或上游圖標。另一方面，`.skills-manager` 路徑、database filename、backup metadata、Git refs／trailers、Keychain service、localStorage keys 與 `skills-manager-cli` 已成為既有資料或介面契約，不能把品牌整理等同於全域字串取代。

本 change 是 Phase 0 fork identity 的後續整理，但排在 Phase 1 `protect-offline-external-library` 完成及 `remove-upstream-app-updater` 實作後，避免改動目前收尾 change。AgentDeck 不對外發布；Bundle ID 仍需穩定，因 macOS 用它區分 App container、日誌與權限。核心 Library 與 SQLite 目前由 `central_repo` 明確路徑解析，不應因 Bundle ID 變更而移動或重建。

## Goals / Non-Goals

**Goals:**

- 讓使用者看到的 desktop App、bundle metadata、主程式 package 與主要 repository overview 一致使用 `AgentDeck`。
- 將主 App Bundle ID 固定為 `io.github.yichin17.agentdeck`，並驗證核心資料解析不依賴舊 Bundle ID。
- 使用不含上游圖像的 AgentDeck icon source，產生 macOS、Windows 與通用 desktop assets。
- 為 macOS 產生單色、透明背景且以 template mode 顯示的 Tray icon。
- 保留持久資料、backup protocol、Keychain、localStorage 與 Skill CLI 的 legacy compatibility identifiers。
- 建立明確 allowlist 的自動檢查，阻止使用者可見品牌回退為 Skills Manager。

**Non-Goals:**

- 不改 `.skills-manager`、`skills-manager.db`、backup metadata、Git refs／trailers、Keychain service 或 localStorage key。
- 不改 `skills-manager-cli` 名稱、參數、輸出 schema 或 `skills/manage-skills/SKILL.md` 的 command contract。
- 不改 GitHub OAuth client ID 或 GitHub 授權頁顯示的外部 OAuth App 名稱。
- 不移除 updater、不建立 release／signing／notarization；updater 由前一 change 處理。
- 不改 SQLite schema、Library location、外部 Library identity 或 Artifact model。
- 不重新編寫完整 README、歷史 CHANGELOG、上游 Simplified Chinese README 或 baseline evidence。

## Decisions

### AgentDeck 成為唯一 display name 並使用穩定 Bundle ID

Tauri `productName`、window title、package metadata、HTML title、App menu／Tray、使用者可見 i18n、診斷標頭及主要 README 使用 `AgentDeck`；主 Rust／Cargo package 與 desktop binary 使用 `agentdeck`。Bundle ID 固定為 `io.github.yichin17.agentdeck`，不以版本、裝置或開發環境改寫。

替代方案是只換視窗標題並保留上游 package／Bundle ID；系統層、Dock、日誌與建置產物仍會顯示舊身份，因此不採用。另一替代方案是使用暫時 Bundle ID；再次變更會重複產生 App container 與權限切換，因此不採用。

### AgentDeck icon 使用單一原始圖稿產生 desktop assets

App icon 採不含文字的「四張層疊 Artifact cards／deck」幾何圖形，使用高對比的藍紫色系與簡潔輪廓，在 16–1024 px 仍能辨認，不沿用或描摹上游圖標。保留一份 1024×1024 或更高的無損 master source，透過 Tauri 既有 icon tooling 產生 `.icns`、`.ico`、通用 PNG 與 Windows Square assets，README 使用同一來源縮圖。

macOS Tray 另用相同輪廓的單色透明版本，啟用 template mode 讓系統依深色／淺色選單列著色；其他平台使用非 template asset，並在該平台預設深色與淺色 system tray theme 人工驗證輪廓可辨識。

替代方案是直接縮小彩色 App icon 作為 Tray icon；小尺寸與深色模式辨識不足，因此不採用。另一替代方案只替換 `.icns`；Windows、development window 與 README 仍會顯示舊圖，因此不採用。

### Legacy persistence 與 protocol identifiers 保持不變

以下名稱視為相容識別而非品牌文案：`.skills-manager` storage／metadata、`skills-manager.db`、`refs/skills-manager/*`、`Skills-Manager-*` trailers、`skills-manager-git-backup` Keychain service、既有 localStorage keys 與 `skills-manager-cli`。本 change 不搬移、不複製、不刪除這些資料，也不改 backup protocol。

Bundle ID 切換前以 focused tests／path assertions 固定核心 Library、SQLite、config 與 Keychain lookup 仍解析到既有位置。Tauri-managed logs、WebView cache 與未同步到 backend 的純視覺偏好不屬核心資料；新 bundle 可建立新 container。實作交付時須明示舊 `.app` 不會自動刪除，使用者需在確認新 App 正常後自行移除。

替代方案是全域 replace `skills-manager`；會造成 Library 看似消失、backup protocol 分裂、Git credential 無法讀取及 CLI 破壞，因此不採用。

### Product identity check 使用明確 surfaces 與 legacy allowlist

新增無第三方 runtime dependency 的 Node check，對 Tauri config、Cargo／npm metadata、HTML title、Rust display strings、en／zh-TW App-owned translations與主要 README 斷言 AgentDeck identity。檢查同時驗證 Bundle ID 精確值、主 icon master 與舊 master hash 不同、必要 desktop outputs 存在且非空。

allowlist 僅包含已列出的 attribution、historical docs、legacy storage／protocol、OAuth 外部名稱與 CLI contract；新命中必須明確加入 allowlist 並說明相容理由，不能用忽略整個目錄逃避檢查。

替代方案是掃描 repository 內所有 `skills-manager`；會把合法 attribution 與 protocol 誤判。只做人工 review 又無法防止 upstream merge 帶回顯示字串，因此兩者都不採用。

### OAuth 與 Skill CLI 保留為明確例外

GitHub device flow 使用的 OAuth App client ID 與授權頁名稱是外部整合身份；沒有 AgentDeck-owned OAuth App 決策前不得改成不存在的名稱。UI 在指引使用者撤銷授權時必須保留 GitHub 實際顯示的名稱，並可標示它是 legacy integration。`skills-manager-cli` 仍是目前 Skill 專用契約，待 Phase 3 Artifact model 確定後再評估 `agentdeck-cli`，避免先改名再重做 command surface。

## Implementation Contract

- **Display identity:** macOS App、Dock／bundle display metadata、main window、App menu、Tray、HTML title、Settings version／diagnostics及主要 README header 顯示 `AgentDeck`；這些 surfaces 不得以 `Skills Manager` 作為產品名稱。
- **Package identity:** npm package 與 Cargo package／default desktop binary 使用 `agentdeck`；Rust library crate `app_lib` 可保留為內部識別；`skills-manager-cli` binary 及其 runner contract 維持不變。
- **Bundle identity:** production 與 development Tauri build 都解析為 `io.github.yichin17.agentdeck`，版本更新不得改變此值。切換後 macOS 可同時看見舊、新 `.app`，AgentDeck 不自動刪除舊 bundle。
- **Core-data preservation:** Bundle ID 切換前後，既有 Library root、SQLite、central repo config、Git backup metadata 與 Keychain service path／name不變；App 不建立新的空 Library 取代既有 Library。外部 Library offline 時仍遵循 `external-library-availability`，不因品牌切換執行 fallback 或 mutation。
- **Ephemeral state:** 不遷移舊 Bundle ID 所屬的 logs、WebView cache；若少數只存在 localStorage 的視覺排序／dismissed flags 無法由新 container 取得，可回到既有預設值，但不得影響 Library、部署狀態、backup 或 secrets。已存在的 localStorage key 名稱保持不變，避免同一 container 內的無謂重置。
- **Icon contract:** master icon 是 AgentDeck-owned、正方形、至少 1024×1024、無文字且與上游 master 不同；required PNG、`.icns`、`.ico` 與 Windows Square outputs 由同一 master 產生。macOS Tray icon 使用單色透明 asset 並以 template mode 顯示；16 px、32 px、128 px、macOS Dock 及深／淺色 Tray 人工檢查均可辨識。
- **Exception contract:** attribution／license／baseline 可保留上游名稱；`.skills-manager` storage／protocol、`skills-manager-cli`、OAuth 真實外部名稱與 historical changelogs 可保留。使用者可見的一般產品文案不得以 legacy 例外為由保留上游品牌。
- **Regression check:** product identity check 必須對指定 surfaces 的錯誤 display name、錯誤 Bundle ID、舊 icon hash或缺失 desktop asset 非零結束並指出位置；合法 allowlist 命中必須通過。
- **Acceptance criteria:** `npm run build`、`npm run lint`、`npm run check:i18n`、product identity check、`cargo test --manifest-path src-tauri/Cargo.toml` 與 Tauri build metadata inspection 成功；人工檢查 Dock、window、App menu、Tray、Settings 與圖標尺寸。以既有 internal Library、external online Library 及 external offline Library 各啟動一次，確認資料與 offline guard 不變。
- **In scope:** desktop display／package／Bundle identity、icon assets、有限文案與 README header、compatibility assertions及 plan identity 記錄。
- **Out of scope:** persistent protocol rename、CLI redesign、OAuth ownership、updater、public distribution、database／Artifact changes及完整歷史文件重寫。

## Risks / Trade-offs

- [Risk] Bundle ID 變更讓 macOS 同時保留舊 App 並建立新 logs／WebView container → Mitigation：不自動刪除舊 bundle；交付時提供人工確認與移除步驟，核心資料使用明確既有路徑。
- [Risk] 全面 branding scan 破壞 protocol compatibility → Mitigation：使用限定 surfaces 和逐項 allowlist，tests 固定 core paths、Keychain service、CLI 與 backup identifiers。
- [Risk] 新 icon 小尺寸難辨識或 Tray 在深色模式消失 → Mitigation：App 與 Tray 分開輸出，在 16／32 px及 macOS 深／淺色模式人工驗證。
- [Risk] Cargo package rename破壞 CLI runner → Mitigation：保持 `skills-manager-cli` explicit bin 名稱，執行 `npm run cli:build` 與 CLI smoke test。
- [Risk] 上游 merge重新帶回 display strings／icons → Mitigation：將 product identity check 納入 change 驗收，命中時要求明確分類為品牌或相容例外。

## Migration Plan

1. 先新增 core-data／legacy identifier tests 與 product identity failing checks，記錄舊 icon master hash。
2. 切換 display／package／Bundle metadata，更新有限使用者文案，保持 legacy identifiers 不變。
3. 建立 AgentDeck icon master，產生 desktop／Tray assets並完成尺寸與深淺模式檢查。
4. 以既有 Library 與 backup／Keychain 設定啟動新 Bundle ID build，驗證資料可用；關閉舊 App，避免兩個 bundle 同時寫入相同 Library。
5. 確認新 App 穩定後，由使用者人工刪除舊 `Skills Manager.app`；不得由 migration code 自動刪除。
6. rollback 還原 metadata 與 icons 後仍讀相同核心資料；保留新舊 logs／WebView container，不做破壞性清理。

## Open Questions

無；display name、Bundle ID、icon方向與 legacy compatibility 邊界已在本提案定案。OAuth ownership 與 CLI redesign 明確延後到各自 change。
