## Why

這是 `plan.md` Phase 1「Codex 路徑與 Library 基礎」尚未完成的最後一項。當使用者把中央 Library 放在外接磁碟，而磁碟在啟動或執行期間未掛載時，現有路徑初始化可能在原掛載點建立空目錄並把它當成新 Library；後續 reindex、同步或刪除流程因此可能把「來源離線」誤判成「內容已刪除」，違反資料安全原則。

## What Changes

- 將可移動的 Library content root 與固定在內部磁碟的 App state（SQLite、設定、cache、logs）分離，沿用既有資料時提供可回復、可重試的相容遷移，不改 database schema。
- 對已設定的外部 Library 執行不產生目錄的可用性探測；掛載點缺失、目標不可讀／不可寫或識別不符時進入明確的 `offline` 狀態，不 fallback 到預設 Library，也不建立替代空 Library。
- 提供 frontend 可查詢及重試的 Library availability contract，離線時在 `Layout` 共用外殼顯示 `Library Offline` 與設定路徑（board route 與一般 route 都可見），保留可安全讀取的內部狀態，但停用會改動 Library、部署目標、metadata 或 Git backup 的操作，包含四欄 Artifact Board 的 lane 拖曳與 Skill 包 add／remove。
- 將 install／import／reimport、update、delete、scenario／Preset sync、Agent／Project sync、metadata reindex/write、file watcher 與自動／手動 Git backup 接到同一個 fail-closed guard，避免各 flow 自行推測路徑狀態。
- 外部 Library 恢復後，使用者可按 Retry 重新探測；只有確認原 Library 可用後才重啟 watcher、重新整理 metadata 與解除動作禁用，期間不自動清除資料庫紀錄或部署目標。
- 新增 offline startup、runtime disconnect、reconnect、legacy config migration 與 primary operations regression tests，並以暫時目錄模擬外部 volume 消失／恢復。

## Non-Goals

- 不自動掛載磁碟、猜測改名後的 volume、複製外部 Library 到預設位置或建立背景離線寫入佇列。
- 不在離線期間允許新增、更新、刪除、同步或 Git backup；read-only cached inventory 不代表檔案內容可開啟。
- 不修改 Agent Skill 路徑、project workspace routing、sync mode、database schema 或 Git backup protocol。
- 不改動已存在的四欄 Artifact Board／docked Inspector 版面、lane 定義與互動設計（由已歸檔的 `default-product-ui-to-traditional-chinese` 建立），只在其上套用 offline banner 與 mutation 禁用；也不擴充 Plugin、Hook 或 Config Profile 模型。
- 不新增或恢復語系：`src/i18n` 維持 `en` 與 `zh-TW` 兩個 locale，不重建已移除的 `zh.json`。

## Capabilities

### New Capabilities

- `external-library-availability`: 定義外部 Library 與內部 App state 的邊界、online／offline 探測、fail-closed actions、reconnect 與相容遷移行為。

### Modified Capabilities

(none)

## Impact

- Affected plan: `plan.md` Phase 1，完成 Library offline 防護後才進入 Phase 2。
- Affected specs: `external-library-availability`
- Intentional upstream divergence: AgentDeck 允許外部 Library 且必須在 volume 離線時維持 App 可啟動與資料庫安全；上游既有單一路徑初始化行為不再直接套用到外部 Library。
- Affected code:
  - Modified: `src-tauri/src/core/central_repo.rs`
  - Modified: `src-tauri/src/core/error.rs`
  - Modified: `src-tauri/src/core/app_state.rs`
  - Modified: `src-tauri/src/core/installer.rs`
  - Modified: `src-tauri/src/core/scenario_service.rs`
  - Modified: `src-tauri/src/core/sync_metadata.rs`
  - Modified: `src-tauri/src/core/file_watcher.rs`
  - Modified: `src-tauri/src/core/auto_backup.rs`
  - Modified: `src-tauri/src/commands/settings.rs`
  - Modified: `src-tauri/src/commands/agent_workspace.rs`
  - Modified: `src-tauri/src/commands/projects.rs`
  - Modified: `src-tauri/src/commands/presets.rs`
  - Modified: `src-tauri/src/commands/skills.rs`
  - Modified: `src-tauri/src/commands/sync.rs`
  - Modified: `src-tauri/src/commands/git_backup.rs`
  - Modified: `src-tauri/src/lib.rs`
  - New: `src-tauri/src/core/library_availability.rs`（identity marker、無副作用 probe、runtime availability state 與 `ensure_library_online()` guard；不放進已達 1700 行的 `central_repo.rs`）
  - Modified: `src/lib/tauri.ts`
  - Modified: `src/lib/error.ts`
  - Modified: `src/context/AppContext.tsx`
  - Modified: `src/components/Layout.tsx`
  - Modified: `src/components/ArtifactBoard.tsx`
  - Modified: `src/components/PresetBar.tsx`
  - Modified: `src/views/MySkills.tsx`
  - Modified: `src/views/ProjectDetail.tsx`
  - Modified: `src/views/Settings.tsx`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - New: `src/components/LibraryOfflineBanner.tsx`
- Affected checks: 新增的 i18n key 必須通過 `npm run check:i18n`（en／zh-TW leaf key 與 placeholder parity、zh-TW 台灣術語表）；改動 board 與 Skill 包元件後必須維持 `npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui` 通過。
- Dependencies: 不新增 npm package 或 Rust crate；沿用現有 path guard、SQLite、Tauri state、watcher 與 `StatusBanner` 基礎設施。
