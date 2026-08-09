## Context

`src/i18n/index.ts` 原本依序讀取 backend setting 與 local storage，並把 `zh` 視為有效語言。初版實作雖已把缺省值與 `fallbackLng` 改成 `zh-TW`，但實機 App data 若仍保存舊 `zh`，啟動時依然會載入簡體中文，設定頁也能再次選取 `zh`。`src/i18n/zh-TW.json` 已涵蓋主要畫面並完成台灣用詞校訂，但產品要全面採用繁體中文，就不能繼續散布或選取簡體中文資源。

`src/hooks/useTheme.ts` 在 local storage 沒有有效值時回傳 `dark`，因此首次 render 會套用深色主題；`src/index.css` 雖已定義 light tokens，實際產品預設並未使用它們。現有 `Layout` 提供左側導覽與中央內容，但 Library／Project 畫面主要是一般 grid／list，`DetailSheet` 會從 sidebar 右側覆蓋整個內容區。這與 `plan.md` Phase 2 及使用者提供的參考圖所定義的「固定左側導覽、中央四欄 Board、固定右側 Inspector」不一致。

現有資料已能表達 Skill 與 Agent target 關係，現有 `@dnd-kit` 依賴也能處理拖曳，因此本 change 不需要新增資料表、IPC payload 或第三方套件。初版 Board 實機驗收顯示 sticky 工具列與其後的來源／標籤篩選器分屬不同區塊，負 margin 會讓篩選器靠入工具列邊界；頁面捲動時 sticky top offset 留出的區域也會讓下方 lane 與卡片穿過標題層。這個 change 會刻意偏離上游的簡體中文、深色預設與一般列表主畫面，但保留英文、主題、跨平台與其他 Agent 行為。

後續實機驗收也確認兩項語意問題。第一，中央 Library 與 Project Board 的 `false/false` 欄都顯示為 Library，但 Project 的實際行為只是把 Skill 保留在專案的停用目錄，並未匯入中央技能庫。第二，使用者介面的 `Preset` 實際上是可混合任意 Skills 的批次成員清單，不是 App 預設值或獨立副本。Inspector 開啟後還會因固定寬度壓縮 Board，使原本位於 Both 欄的選取卡片離開可見範圍；雖可手動水平捲動，卻無法維持選取脈絡。

## Goals / Non-Goals

**Goals:**

- 只提供 `zh-TW` 與 `en`，把舊 `zh` 值相容解析為 `zh-TW`，並以 `zh-TW` 作為翻譯 fallback。
- 無有效外觀偏好時以 `light` 首次呈現，保留有效 `light`、`dark`、`system` 偏好。
- 建立台灣用詞表與 locale 完整性檢查，不改變既有 key 或 placeholder。
- 讓 Library 與 Project 管理流程以 Library／Codex／Claude／Both 四欄 Board 為預設檢視。
- 讓拖曳與 Inspector checkbox 共用同一套 canonical target 更新規則，且不建立 Artifact 副本。
- 讓 Board、固定 Inspector、左側導覽與上方工具列呈現參考圖的淺色、清楚、低干擾視覺層級。
- 讓 Library／Project 的 sticky 標題與工具列形成不透明且連續的頂層，次要篩選器不被遮住，長 Board 捲動時內容不穿過標題區。
- 讓中央 Library、Project 未部署狀態與 Skill 包各自使用符合實際資料作用的名稱，不再把三者混為同一種儲存位置或設定。
- 讓 Inspector 開啟與卡片換欄後仍可直接看到選取卡片所在欄，不要求使用者手動尋找。
- 讓 Skill 包的批次加入與批次移除使用不同動作，並在移除前明確顯示範圍與取得確認。
- 保留 List 檢視、鍵盤替代操作、深色主題、非 canonical Agent target 與現有後端契約。

**Non-Goals:**

- 不依 macOS 系統語言自動選擇 locale，也不新增其他 locale。
- 除了把舊 `zh` 相容解析成 `zh-TW`，不強制遷移既有 `zh-TW`、`en` 或主題偏好，不改 settings schema、database schema 或設定 IPC。
- 不新增 Artifact type、Plugin／Hook／Config Profile 功能、deployment engine 或 Library offline 偵測。
- 不重寫 Agent Skills 專用 discovery／read-only 工作流程，不刪除非 Codex／Claude target。
- 不把原始 `description` 截短後回寫，也不修改使用者的 Skill 文件內容。
- 不讓拖曳至 Project「未部署」觸發中央技能庫匯入、更新或同名衝突解決；這些仍由既有明確中央同步操作負責。

## Decisions

### 只接受繁體中文與英文，舊 zh 相容轉為 zh-TW

語言解析維持 `valid backend setting > valid local storage > zh-TW`，但有效集合縮為 `zh-TW` 與 `en`。任何來源的舊 `zh` 都正規化為 `zh-TW`；設定頁只列出繁體中文與 English，i18next resources 與 locale 完整性檢查也移除 `zh`。`fallbackLng` 維持 `zh-TW`；未知值、空字串與 settings 讀取失敗均安全回到下一層有效值或 `zh-TW`，不顯示啟動錯誤。

替代方案是保留 `zh` resource 但隱藏設定按鈕；這仍會讓舊 backend 或 local storage 值載入簡體中文，無法達成全面繁體中文，因此不採用。

### 保留使用者明確選擇且不做資料 migration

有效的 `zh-TW`、`en` 與 `light`、`dark`、`system` 值維持原意。舊 `zh` 是唯一例外，載入時視為 `zh-TW`，避免使用者停留在已移除的語系；既有 `dark` 不改成 `light`。設定頁沿用現有切換與持久化流程，但不再顯示簡體中文。

替代方案是啟動時全面遷移舊值，但無法判斷該值是上游預設留下或使用者刻意選擇，可能違反使用者偏好，因此不採用。

### 用明確台灣用詞表校訂 zh-TW

使用以下產品用詞：`local` 對應「本機」、repository 對應「儲存庫」、application 對應「App」、project 對應「專案」、settings 對應「設定」、global 對應「全域」、read-only 對應「唯讀」、import/export 對應「匯入／匯出」。`Skill`、`Agent`、`Library`、Git、CLI、API、JSON、TOML 等領域或技術名詞保留原文。

校訂以使用者可見值為範圍，不更名 translation keys，也不改變 `{{name}}`、`{{count}}` 等插值 placeholder。Board 新文案只需在 `en`、`zh-TW` 保持一致，不藉此改寫英文內容。

### 用標準函式庫檢查 locale 契約

新增 `scripts/check-i18n-locales.mjs`，遞迴攤平 JSON 後檢查：正式 resources 的 leaf keys 集合一致、同一 key 的 placeholder 集合一致、台灣用詞表列出的禁用詞不出現在 `zh-TW` 值中。`package.json` 提供固定 npm script；不新增 test framework 或 npm dependency。

用詞檢查只包含已決定且能無歧義替換的完整詞彙，不用「偵測所有簡體字」的字元表，避免把兩岸共用字或專有名詞誤判。

### 無有效外觀偏好時以 light 首次呈現

主題解析維持現有架構：初始 render 先使用有效 local storage，沒有有效值時使用 `light`；backend 回傳有效值後再以 backend 為準並同步 local storage。`system` 仍透過 `prefers-color-scheme` 解析，使用者明確選擇 `dark` 時仍套用 `.dark`。

light 是缺省值而不是強制主題。`index.css` 的 `:root` light tokens 必須在第一個畫面可直接使用，避免等待 backend 時先閃出深色畫面。

### 以四個互斥欄位呈現 canonical target 狀態

Board 只以 Codex 與 Claude 兩個 canonical target 推導單一欄位：兩者都未選時，中央 Board 顯示 Library，Project Board 顯示「未部署」；只有 Codex 為 Codex、只有 Claude 為 Claude、兩者皆選為 Both。Artifact 在任一 Board context 只渲染一張卡；欄位移動是 target 狀態轉換，不複製資料或來源檔案。

中央 Board 拖曳到 Library、Project Board 拖曳到「未部署」都會移除 Codex 與 Claude；Project Skill 保留在專案既有停用位置，不因此新增、更新或覆寫中央 Skill。拖曳到 Codex／Claude 會保留對應單一 canonical target，拖曳到 Both 會同時保留兩者。Inspector 的兩個 checkbox 以相同規則重新推導欄位。非 Codex／Claude target 在所有轉換中原樣保留，並在 Inspector 顯示。

### 將 Preset 的產品術語改為 Skill 包

使用者可見的 `Preset`、新增 Preset 與相關說明統一改為「Skill 包」；英文介面使用 `Skill Pack`。Skill 包是中央 Skills 的命名成員清單，可以同時包含不同來源、系列與用途的 Skills。加入或移除包內成員只修改成員關係，不複製 Skill，也不直接變更 Agent 目標；在工作區明確套用 Skill 包時，才以現有批次操作把當下中央版本加入或移出該工作區。

為保留資料與 CLI 相容性，frontend 型別、translation key、Rust command、database scenario/preset 欄位及既有 IPC 名稱維持原名。這次只更正產品術語與導覽狀態，不進行資料 migration。替代方案是把 Skill 包限制為同系列 Skills；這會阻止使用者依工作內容自由組合，因此不採用。

### 讓 Inspector 維持選取卡片可見

`ArtifactBoard` 的水平捲動容器持有各 lane 或 card 的可定位元素。Inspector 從關閉變為開啟，或選取卡片因 target mutation 換欄後，Board 使用瀏覽器原生捲動能力把選取卡片所在欄移入中央可見區域；不改變 lane 固定寬度，也不把 Inspector 疊在 Board 上。關閉 Inspector 不重設水平捲動位置，切換 Board／List 仍遵循既有 view state 契約。

替代方案是縮窄四欄或改用 overlay Inspector；前者會壓縮卡片操作，後者會實際遮住內容，因此不採用。

### 只在技能庫脈絡標示目前 Skill 包

左側 Skill 包的選取樣式代表正在技能庫中查看或編輯該包，不代表全域作用中的工作區。當路由位於 Project、Agent、設定或其他頁面時，Skill 包保留最近查看記錄但不呈現選取背景；目前路由只反白唯一的工作區或導覽項目。這不改變 Skill 包內容、套用狀態或 `viewedPresetId` 的持久化。

### 將 Skill 包批次操作改為明確動作

工作區中的 Skill 包控制不再把整個標籤當作加入／移除 toggle。每個非空 Skill 包提供兩個語意固定的動作：「加入此 Skill 包」只把目前工作區與 Agent 範圍內缺少的 Skill-Agent 項目補齊；「移除此 Skill 包」只移除目前範圍內已存在且屬於該包的 Skill-Agent 項目。兩個動作可位於標籤展開的操作區或相鄰控制，但文字與結果必須在執行前可辨識。

移除是可能刪除工作區部署檔案的動作，因此執行前必須顯示 Skill 包名稱、符合的 Skill-Agent 項目數及不受影響的中央技能庫／包成員提示，並要求確認；取消不得送出 mutation。加入維持可重複執行，已存在項目直接略過。部分失敗沿用既有逐項執行與錯誤數量提示，完成後重新載入 server-confirmed state。

替代方案是依目前狀態讓同一標籤在加入與移除間切換；這使結果必須靠顏色或勾號推測，也讓移除缺少明確確認，因此不採用。

### 用共用 Board view model 隔離既有資料形狀

新增共用的 `ArtifactBoard` presentational component，接收穩定的 frontend view model 與 callback，不直接呼叫 Tauri API。`MySkills` 與 `ProjectDetail` 各自把現有 managed/project Skill 資料轉成 `BoardCardModel`，並使用現有 target 操作完成更新。這讓兩個 caller 共用欄位、卡片、drag-and-drop、空狀態與鍵盤行為，同時保留各頁目前不同的 API 與 identity 規則。

`BoardCardModel` 至少包含 `id`、`title`、`summary`、`artifactType`、`version`、`status`、`canonicalTargets`、`otherTargets` 與可選 icon。`summary` 只供畫面以 CSS line clamp 顯示兩行；沒有摘要時顯示本地化空值，不回寫或截斷原始 description。

### 用固定 Inspector 取代全畫面 DetailSheet

`ArtifactInspector` 使用目前選取卡片的完整資料，在 App shell 右側以固定寬度欄呈現；開啟時中央 Board 重新取得可用寬度，左側 sidebar 保持可操作。Inspector 顯示完整 description、可取得的 when-to-use、canonical 與其他 targets、deployment mode、來源路徑、同步時間／狀態，以及現有 API 能提供的 diff 入口。缺少可選欄位時顯示本地化「未提供」，不捏造內容。

既有 `DetailSheet` 可調整為 docked variant 供 Library／Project 使用；其他仍需 modal sheet 的流程保留 overlay variant。卡片 click／Enter 開啟、`Escape` 或關閉按鈕關閉，拖曳與 checkbox 都不是唯一操作路徑。

### 固定左側導覽與上方工具列維持 Board 脈絡

App shell 保持左側導覽固定，啟動後預設進入 Library Board；選取 Project 則在該 Project context 顯示相同四欄。上方工具列提供 context 名稱、搜尋、同步狀態、Board／List 切換與既有適用操作。側欄只呈現現有可用功能，不為未完成的 Artifact type 建立假入口。

List view 保留為相同資料與操作的替代呈現，切換 view 不改變 target、搜尋、選取或 Inspector 狀態。Agent Skills 專用 workspace 維持原本的 discovery/read-only 版面。

Library 與 Project 的 context title、搜尋、同步與 view controls，以及緊接其後的來源／標籤／Preset filters 必須共同保有明確垂直空間；不得以負 margin 讓次要控制項進入 sticky 區的裁切邊界。sticky 區從可捲動 viewport 的頂端到工具列底部使用不透明背景與高於 Board 內容的堆疊層，長頁面捲動時卡片與 lane 標題只能從其下方出現。

### 淺色視覺 token 統一 surface 與主要操作層級

light tokens 使用白色主 surface、淺灰背景、細中性邊框、低陰影與藍色主要操作；Codex、Claude、Both 欄位分別使用藍、橘、紫的提示色，狀態成功仍使用綠色。dark tokens 保留相同語意與可讀對比，不移除主題切換。

視覺調整透過 `src/index.css` 現有 CSS variables 與 Tailwind classes 完成，不新增 design-system dependency。桌面窄視窗時 sidebar 與 Inspector 保持固定，Board 區域提供水平捲動，欄位不得壓縮到卡片內容無法操作。

## Implementation Contract

- **Observable behavior:** 清除 backend/local 語言與主題設定後啟動 App，首個畫面使用台灣繁體中文與淺色主題；設定頁的語言選項只有繁體中文與 English。Library context 顯示 Library／Codex／Claude／Both 四欄 Board、固定 sidebar 與上方工具列。選取卡片後右側 Inspector 開啟且 Board 脈絡仍可見。
- **Preference contract:** 語言解析順序為有效 backend、有效 local storage、`zh-TW`，有效語言只有 `zh-TW`、`en`；任何舊 `zh` 值解析為 `zh-TW`。主題初始解析為有效 local storage、`light`，backend 有效值載入後優先。有效 `zh-TW`／`en`／`dark`／`system` 不被遷移或覆寫。
- **Board lane contract:** `codex=false, claude=false` 在中央 Board 對應 Library、在 Project Board 對應「未部署」；`true,false` 對應 Codex；`false,true` 對應 Claude；`true,true` 對應 Both。每個 Board context 中每個 Artifact id 只出現一次。拖到 Project「未部署」不得匯入或更新中央技能庫。
- **Mutation contract:** 拖曳或 checkbox 只改 Codex／Claude target，保留其他 target。成功後 card 與 Inspector 使用 server-confirmed state；失敗時回復先前欄位／checkbox、保留選取並顯示本地化錯誤，不留下視覺與持久資料不一致。
- **Inspector contract:** 開啟 Inspector 不遮住 sidebar；顯示可取得的完整 description、when-to-use、targets、deployment mode、source path、sync state 與 diff。缺少可選資料時顯示「未提供」，不得從摘要反推或改寫原始內容。開啟後選取卡片所在欄自動保持可見，關閉時不把 Board 水平捲動重設為起點。
- **Skill pack contract:** 使用者介面稱為「Skill 包」／`Skill Pack`；同一包可包含任意中央 Skills。編輯成員只改成員關係，套用才執行既有一次性批次部署；內部 `Preset` API、型別與資料格式不更名。
- **Navigation contract:** Skill 包只在技能庫路由呈現目前查看狀態；Project 或其他路由不與 Skill 包同時顯示選取背景。
- **Skill pack action contract:** 工作區 Skill 包控制分別提供固定語意的加入與移除動作，不以同一標籤切換。加入只補齊缺少的 Skill-Agent 項目；移除只處理目前工作區與 Agent 範圍內符合項，且在 mutation 前顯示包名與符合項目數並取得確認。取消不得改變檔案、target 或畫面狀態；移除不得刪中央 Skill 或改包成員。
- **View contract:** Board 是 Library 與 Project 的預設；List 使用同一資料、搜尋、target 更新與 Inspector。Agent Skills workspace、其他 Agent target 與 modal dialog 行為保持可用。
- **Sticky layout contract:** Library／Project 工具列與次要篩選器在未捲動時不重疊或裁切；捲動時 sticky 區以不透明背景覆蓋下方內容，lane 標題與卡片不得穿過工具列或出現在其上方。
- **Accessibility contract:** 卡片可用 click／Enter 開啟 Inspector，`Escape` 可關閉；Inspector checkbox 提供拖曳以外的完整 target 操作。active／focus／disabled 狀態在 light 與 dark theme 都可辨識。
- **Verification:** 執行 locale 檢查、Board／layout 檢查、`npm run build`、`npm run lint`、`cargo test --manifest-path src-tauri/Cargo.toml`、Spectra validate/analyze 與 `git diff --check`。以實體 debug `.app` 驗證乾淨設定、舊 `zh` 相容轉換、設定頁只有繁體中文／English、Skill 包可混合成員且只有技能庫路由顯示選取、加入／移除動作分離與移除確認、中央 Library 與 Project「未部署」文案、四種拖曳目的地、Inspector checkbox、失敗 rollback、Board／List、Inspector 自動保持選取欄可見、次要篩選器完整可見、長頁捲動不穿透 sticky 標題、窄視窗水平捲動與 light／dark 對比。
- **In scope:** frontend locale/default theme、移除簡體中文 frontend resource 與選項、light/dark visual tokens、App shell、Library／Project Board、docked Inspector、Skill 包使用者可見術語與導覽選取狀態、既有 target API 串接、繁中／英文新增 UI 文案與可重複驗證。
- **Out of scope:** database/backend schema、內部 Preset／Scenario 命名 migration、Artifact model migration、Project 未部署到中央技能庫的自動匯入與同名衝突 UI、Plugin／Hook／Config Profile 功能、Library offline 偵測、Agent Skills discovery 流程、非 canonical target mutation 與使用者內容改寫。

## Risks / Trade-offs

- [Risk] backend 中既有 `zh` 可能是舊預設或明確選擇，但產品已不允許簡體中文 → 一律相容解析為 `zh-TW`；`dark` 等仍受支援的既有偏好則原樣保留。
- [Risk] 四欄只能完整表達 Codex／Claude，可能讓其他 Agent target 被忽略 → 其他 target 顯示在 Inspector 且任何 Board mutation 都原樣保留；Agent Skills workspace 不改版。
- [Risk] 拖曳過程的 optimistic state 與持久資料失敗後不一致 → target mutation 由頁面 owner 集中處理，失敗回復原 view model 並重新整理 server state。
- [Risk] 固定 sidebar 與 Inspector 壓縮中央空間 → Board 保持固定欄寬並水平捲動，不把 Inspector 改回全畫面遮罩。
- [Risk] Project「未部署」可能被誤認為中央 Library → 使用 context-specific lane label，並讓來源路徑與同步操作維持明確分離。
- [Risk] 使用者可見名稱改成 Skill 包但內部仍叫 Preset → translation key 與 API 保持相容，只在產品文案使用新術語，測試鎖定兩層名稱邊界。
- [Risk] 批次移除會刪除工作區中的多個部署項目 → 執行前顯示精確符合數量與影響邊界，要求確認並讓取消保持零 mutation。
- [Risk] 禁用詞檢查過寬造成合法文案誤報 → 只收錄已決定、可無歧義替換的完整詞彙，例外在 script 中附理由。
- [Risk] 新增 Board 文案造成三語 key 不一致 → locale 檢查以 leaf key 與 placeholder 集合阻擋缺漏。

## Migration Plan

不執行資料或 schema migration。發布後 `zh-TW`、`en`、主題偏好與既有 Preset records 原樣保留；舊 `zh` 在 frontend 載入時相容解析為 `zh-TW`，不需要修改 backend schema。Library／Project 與 Skill 包只改 frontend 呈現；Skill、target 與 Project 資料原樣保留。rollback 還原 frontend、locale 與檢查 script 即可，無資料需要回復。

## Open Questions

無；使用者已確認全面移除簡體中文、預設淺色、Trello 式四欄、固定 sidebar／Inspector、Project `false/false` 為未部署狀態，以及可混合任意 Skills 的「Skill 包」產品術語。
