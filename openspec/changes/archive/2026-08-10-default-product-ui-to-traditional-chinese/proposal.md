## Why

這個 change 原本是 `plan.md` Phase 1 收尾的產品語系修正，現在一併承接已確認的 Phase 2「AgentDeck Board 與 Description」介面方向。初版實作雖把無偏好預設改成 `zh-TW`，實機驗收仍發現既有 `zh` 偏好會讓 App 啟動為簡體中文，設定頁也仍提供簡體中文選項；Board 的 sticky 工具列另有次要篩選器被遮住、長頁面捲動時卡片穿過標題區的層級問題。這些都直接違反本 change 的預設繁中與可用 Board 契約，必須在封存前修正並以實體 `.app` 驗證。

## What Changes

- 產品只提供 `zh-TW` 與 `en`；移除簡體中文資源與設定選項，既有或無效的 `zh` 偏好一律解析為 `zh-TW`，翻譯 key 缺失時也 fallback 到 `zh-TW`。
- 保留使用者已明確儲存的 `zh-TW` 或 `en` 選擇，維持 backend 設定優先於 local storage 的既有順序；唯一語言相容轉換是把舊 `zh` 值視為 `zh-TW`，避免任何啟動路徑再次顯示簡體中文。
- 依台灣用詞表校訂 `zh-TW` 文案，統一使用「本機、儲存庫、App、專案、設定、全域、唯讀、匯入」等詞彙，保留 `Skill`、`Agent`、`Library`、API 與命令等技術名詞。
- 新增不依賴第三方套件的 locale 完整性檢查，驗證 `zh-TW` 與基準 locale 的 key／插值 placeholder 一致，並防止已定義的非台灣用詞重新進入繁體中文資源。
- 全新安裝或外觀設定無效時預設使用淺色主題；保留既有 `light`、`dark`、`system` 選擇與設定頁切換能力，不強制改寫既有深色偏好。
- 將 Library 與 Project 的主要管理畫面改為淺色 Trello 式 Board，使用 Library／Codex／Claude／Both 四欄、卡片、水平捲動、清楚的欄位色彩提示與上方搜尋／同步／Board／List 工具列。
- 使用既有 drag-and-drop 依賴讓卡片能在四欄之間移動；也能從右側固定 Inspector 勾選 Codex／Claude 目標。兩種操作更新同一筆 Artifact 的既有 target 狀態，不建立重複資料，失敗時回復畫面並顯示錯誤。
- 卡片只顯示可快速掃描的兩行摘要與必要狀態；固定右側 Inspector 顯示完整 description、when-to-use、targets、deployment mode、來源路徑、同步狀態與可用的 diff，不再以全內容區遮罩阻斷 Board 脈絡。
- 調整 App shell 的視覺層級：固定左側導覽、中央 Board、右側 Inspector；採白色／淺灰 surface、細邊框、低陰影與藍色主要操作色，深色主題使用相同語意 token 保持可用。
- 修正 Library／Project sticky 工具列的版面與堆疊層級：來源／標籤等次要篩選器在初始位置完整可見，捲動長 Board 時 lane 標題與卡片只在工具列下方繪製，不得穿過或出現在工具列上方。
- 將使用者介面的 `Preset` 改稱「Skill 包」：每個 Skill 包可自由收納不同來源、不同用途的中央 Skills，作為可重複套用至全域或專案 Agent 的批次清單；內部既有 `Preset` 型別、資料表與 IPC 名稱維持不變。
- 區分中央 Library 與 Project 的未部署狀態：中央 Board 保留 Library 欄，Project Board 的 `false/false` 欄改稱「未部署」，拖入該欄只取消 Codex／Claude 部署並保留專案 Skill，不匯入或更新中央技能庫。
- Inspector 開啟或選取卡片換欄後，Board 自動讓選取卡片所在欄保持在可見範圍；關閉 Inspector 時保留使用者原有的 Board 水平捲動脈絡。
- 在專案頁只反白目前專案，不讓先前查看的 Skill 包同時呈現導覽選取狀態，避免把 Skill 包誤認為第二個作用中工作區。
- 將工作區的 Skill 包批次操作改成分離且明確的「加入此 Skill 包」與「移除此 Skill 包」；不再以同一個標籤點擊暗中切換相反動作，移除前顯示符合的 Skill-Agent 項目數並要求確認。
- 保留 List 檢視、鍵盤操作、既有非 Codex／Claude target，以及 Agent Skills 專用工作流程；Board 的 canonical 欄位變更不得刪除其他 Agent target。

## Non-Goals

- 不移除英文或深色／跟隨系統主題，也不改寫已明確儲存的 `zh-TW`、`en` 與外觀偏好。
- 不翻譯或改寫 Skill／Agent 使用者內容、`SKILL.md`、程式碼識別字、檔案格式、CLI 或 API。
- 不新增 Plugin、Hook、Config Profile 等尚未實作的管理功能或空白頁面；側欄只重整現有可用入口。
- 不修改 backend settings schema、database schema、Artifact identity、部署格式、同步規則或官方 Plugin cache。
- 不把 Project「未部署」欄改成專案到中央技能庫的匯入入口，也不在本 change 新增同名異內容 Skill 的覆寫／另存衝突對話框；中央同步維持既有明確操作。
- 不改變 Skill 包批次加入／移除的底層部署範圍：加入仍只補齊缺少項，移除仍只處理目前工作區與所選 Agent 中符合該包的部署；不刪中央 Skill 或 Skill 包成員關係。
- 不在本 change 實作 Library offline 偵測；若其他 change 已提供離線狀態，Board 必須沿用其寫入封鎖行為。

## Capabilities

### New Capabilities

- `product-localization-defaults`: 定義 AgentDeck 的預設語言、fallback、既有偏好保留、台灣用詞與 locale 完整性契約。
- `product-board-interface`: 定義預設淺色外觀、四欄 Board、target 變更、固定 Inspector、List 相容與非 canonical target 保留契約。

### Modified Capabilities

(none)

## Impact

- Affected specs: `product-localization-defaults`, `product-board-interface`
- Plan phase: `plan.md` Phase 1 語系收尾與 Phase 2「AgentDeck Board 與 Description」。
- Intentional upstream divergence: AgentDeck 移除上游簡體中文資源與設定選項，將舊 `zh` 偏好相容轉為台灣繁體中文、無偏好預設外觀由深色改為淺色，主要 Artifact 管理流程改為 AgentDeck 的四欄 Board；英文、主題與非 canonical Agent 能力仍保留。
- Affected code:
  - Modified: `src/i18n/index.ts`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - Modified: `src/hooks/useTheme.ts`
  - Modified: `src/App.tsx`
  - Modified: `src/index.css`
  - Modified: `src/components/Layout.tsx`
  - Modified: `src/components/Sidebar.tsx`
  - Modified: `src/components/DetailSheet.tsx`
  - Modified: `src/components/ArtifactBoard.tsx`
  - Modified: `src/components/PresetBar.tsx`
  - Modified: `src/views/Settings.tsx`
  - Modified: `src/views/MySkills.tsx`
  - Modified: `src/views/ProjectDetail.tsx`
  - New: `src/components/ArtifactInspector.tsx`
  - Modified: `package.json`
  - New: `scripts/check-i18n-locales.mjs`
- Dependencies: 使用既有 `@dnd-kit`、React、Tailwind CSS 與 Tauri settings API；不新增 npm package、database migration 或 backend IPC contract。`Preset` 僅變更使用者可見術語，既有資料與 API 相容。
