## 1. Locale 完整性與台灣用詞

- [x] 1.1 依照「用標準函式庫檢查 locale 契約」先建立 `scripts/check-i18n-locales.mjs` 與 `package.json` 的固定命令，交付「Locale resources preserve structural integrity」：遞迴檢查正式 locale 的 leaf keys、`{{placeholder}}` 集合與明確禁用詞，錯誤輸出包含 translation key 與差異；在修改 `src/i18n/zh-TW.json` 前執行該 npm script，確認目前的非台灣用詞使它以非零狀態失敗。
- [x] 1.2 依照「用明確台灣用詞表校訂 zh-TW」完成「Traditional Chinese uses Taiwan product terminology」：逐一校訂 AgentDeck-owned `src/i18n/zh-TW.json` 既有值，統一「本機、儲存庫、App、專案、設定、全域、唯讀、匯入／匯出」，保留技術名詞、translation keys 與 placeholders；以 locale 完整性 npm script exit 0 及 `git diff` 人工檢查證明沒有全面改寫 `src/i18n/zh.json`、`src/i18n/en.json` 或任何使用者內容，兩個其他 locale 只允許在後續 Board 任務加入對應新 key。

## 2. 預設語言、外觀與偏好保留

- [x] 2.1 依照「以有效持久偏好優先，缺省與 fallback 統一為 zh-TW」及「保留使用者明確選擇且不做資料 migration」完成「Traditional Chinese is the product default」與「Explicit supported language preferences are preserved」：`src/i18n/index.ts` 維持有效 backend setting、有效 local storage、`zh-TW` 的解析順序並把 `fallbackLng` 設為 `zh-TW`，不改寫有效 `zh`／`en`；以 `npm run build` 及 debug bundle 人工矩陣驗證無偏好、無效偏好、backend 優先、local storage fallback、手動切換與重啟持久化。
- [x] 2.2 依照「無有效外觀偏好時以 light 首次呈現」及「保留使用者明確選擇且不做資料 migration」完成「Light theme is the product appearance default」：`src/hooks/useTheme.ts` 在無有效 local value 時以 `light` 首次 render，仍讓有效 backend value 優先並保留 `dark`／`system`；以乾淨設定、三種有效偏好、無效值與系統外觀變更的 debug bundle 矩陣驗證首屏 class 與重啟結果。

## 3. App shell 與淺色 Board 視覺

- [x] 3.1 依照「淺色視覺 token 統一 surface 與主要操作層級」更新 `src/index.css` 的 light／dark semantic tokens 與共用元件 class，交付「App shell uses the light Board visual hierarchy」：淺色使用中性 surface、細邊框、低陰影與藍色 action，Codex／Claude／Both 使用藍／橘／紫提示；以 light／dark 畫面人工檢查 focus、active、disabled、status 與文字對比，且 `package.json` 不新增 UI dependency。
- [x] 3.2 依照「固定左側導覽與上方工具列維持 Board 脈絡」調整 `src/App.tsx`、`src/components/Layout.tsx`、`src/components/Sidebar.tsx` 與 Library／Project header，使啟動預設進入 Library Board、固定 sidebar 與 top toolbar 保持可用、Project 進入對應 Board context，且不顯示未實作功能入口；以路由巡覽、搜尋、同步狀態、Board／List 切換與窄視窗人工矩陣驗證，確認 Agent Skills workspace 與 Settings 仍可到達。

## 4. 四欄 Board 與 target 更新

- [x] 4.1 依照「以四個互斥欄位呈現 canonical target 狀態」及「用共用 Board view model 隔離既有資料形狀」新增 `src/components/ArtifactBoard.tsx`，完成「Artifact management defaults to a four-lane Board」與「Cards remain concise and preserve source content」：定義 `BoardCardModel` 與 Library／Codex／Claude／Both 推導，讓 `src/views/MySkills.tsx`、`src/views/ProjectDetail.tsx` 各自轉接既有資料；以四組 Codex／Claude boolean fixture、重複 id guard、兩行 summary、空摘要與非 canonical target fixture 的可重複檢查及 `npm run build` 驗證每個 context 每個 identity 只產生一張卡且不改原始 description。
- [x] 4.2 依照「以四個互斥欄位呈現 canonical target 狀態」串接既有 `@dnd-kit` 與 target APIs，完成「Board target changes use drag and Inspector controls」的 drag path：四個目的欄分別持久化 `false/false`、`true/false`、`false/true`、`true/true`，保留其他 Agent target；以 temporary project／Library 資料逐一拖曳四個目的地並重載確認同一 Artifact 未被複製、target 正確、其他 target 未消失。
- [x] 4.3 讓 target mutation owner 在 API 成功後採用 confirmed state，失敗時重新整理並回復原 lane、保留選取且顯示本地化錯誤，完成「Failed target update restores the confirmed state」；以攔截失敗的 debug flow 驗證 card、Inspector checkbox 與持久資料一致，並確認 drop 回原欄不送出 mutation。

## 5. 固定 Inspector、List 與既有流程相容

- [x] 5.1 依照「用固定 Inspector 取代全畫面 DetailSheet」新增 `src/components/ArtifactInspector.tsx` 並擴充 `src/components/DetailSheet.tsx` 的 docked variant，完成「Selected Artifact opens a docked Inspector」：讓 Library／Project card 以 click／Enter 開啟右側 Inspector，顯示可取得的完整 description、when-to-use、targets、deployment mode、source path、sync state 與 diff，缺值顯示本地化 unavailable、`Escape` 關閉；以有／無選填資料、可用／不可用 diff、checkbox 更新與窄視窗人工矩陣確認 sidebar／Board 不被遮住。
- [x] 5.2 依照「固定左側導覽與上方工具列維持 Board 脈絡」完成「Board and List share state and operations」與「Existing specialized workflows remain available」：`src/views/MySkills.tsx`、`src/views/ProjectDetail.tsx` 的 Board／List 共用搜尋、identity、selection、target mutation 與 Inspector，切換不送 mutation；以 debug bundle 驗證 view 切換後 filter／selection 保留、Agent Skills read-only actions 未變、其他 Agent target 未被 Board 操作移除、既有 modal dialogs 仍正常。
- [x] 5.3 在 `src/i18n/en.json`、`src/i18n/zh.json`、`src/i18n/zh-TW.json` 加入 Board lane、toolbar、Inspector 欄位、unavailable 與 mutation error 的對應文案，保持 key 與 placeholders 完全一致；以 locale 完整性 npm script exit 0 及三語 debug 畫面確認沒有 raw key、缺字或截斷按鈕。

## 6. 實機驗收回饋修正

- [x] 6.1 依照「只接受繁體中文與英文，舊 zh 相容轉為 zh-TW」完成「Explicit supported language preferences are preserved」：從 `src/i18n/index.ts` resources、`src/views/Settings.tsx` 選項與 locale 完整性輸入移除簡體中文，刪除 `src/i18n/zh.json`，讓 backend 或 local storage 的舊 `zh` 值只載入 `zh-TW`；以 locale 完整性命令、`npm run build` 與實體 debug `.app` 驗證設定頁只有繁體中文／English，乾淨、舊 `zh`、`zh-TW`、`en` 四種狀態皆載入預期語系且畫面沒有簡體中文。
- [x] 6.2 依照「固定左側導覽與上方工具列維持 Board 脈絡」與 Sticky layout contract 修正 Library／Project 工具列間距、背景及堆疊層，交付「App shell uses the light Board visual hierarchy」：未捲動時來源／標籤／Preset filters 完整位於工具列下方，長 Board 捲動時 lane 標題與卡片不出現在 sticky 區上方或穿過其背景；以實體 debug `.app` 分別在 Library／Project、一般／窄視窗與長內容頁面捲動驗證。
- [x] 6.3 依照「將 Preset 的產品術語改為 Skill 包」先擴充 Board／layout 與 locale 可重複檢查，交付「Skill Packs are reusable mixed-skill collections」的失敗測試：要求繁中顯示「Skill 包」、英文顯示 `Skill Pack`、內部 `Preset` API／型別名稱仍存在，且 Project `false/false` lane label 與中央 Library label 不同；在實作前執行相關 npm checks，確認至少一項新 assertion 以非零狀態失敗。
- [x] 6.4 依照「將 Preset 的產品術語改為 Skill 包」及「以四個互斥欄位呈現 canonical target 狀態」更新使用者可見文案與 Board context labels，交付「Skill Packs are reusable mixed-skill collections」與「Project undeployed lane remains project-local」：繁中介面統一使用「Skill 包」、英文使用 `Skill Pack`，中央 Board 保留 Library、Project Board 顯示「未部署」，拖入未部署只切換既有 Codex／Claude targets；以 locale／Board checks、`npm run build` 及 debug `.app` 驗證混合 Skill 成員清單不複製中央內容、Project 未部署不改變中央技能庫數量。
- [x] 6.5 依照「讓 Inspector 維持選取卡片可見」完成「Selected lane remains visible when Inspector opens」：Inspector 開啟或選取卡片換欄後，Board 以原生水平捲動讓該卡片所在 lane 保持可見，關閉 Inspector 不重設 scroll state，且不改為 overlay 或壓縮固定 lane；以 layout check、`npm run build` 與實體 debug `.app` 在 Both、Library／未部署、Codex、Claude 四個 lane 逐一驗證。
- [x] 6.6 依照「只在技能庫脈絡標示目前 Skill 包」完成「Skill Pack selection is scoped to the Library」：左側 Skill 包只在 `/my-skills` 顯示目前查看樣式，Project／Agent／Settings 等路由只反白各自 context，回到技能庫仍恢復最近查看的 Skill 包；以可重複 layout check、`npm run build` 及 debug `.app` 路由巡覽驗證同一時間只有一個導覽 context 呈現選取狀態。
- [x] 6.7 依照「將 Skill 包批次操作改為明確動作」先擴充可重複 UI contract 檢查，交付「Skill Pack deployment actions are explicit and safe」的失敗測試：要求非空 Skill 包同時存在固定語意的加入與移除入口、標籤本身不直接送出 mutation、移除確認包含包名與精確 Skill-Agent 符合數量、取消為零 mutation；在實作前執行對應 npm check，確認新 assertion 以非零狀態失敗。
- [x] 6.8 依照「將 Skill 包批次操作改為明確動作」完成「Skill Pack deployment actions are explicit and safe」：`PresetBar` 不再以同一標籤切換加入／移除，加入只補缺少項，移除前顯示 Skill 包名稱、精確符合數量及中央 Skill／成員不受影響提示並要求確認，取消不送 mutation，確認只移除目前 workspace／Agent 範圍的符合項；以 UI contract check、`npm run build` 及實體 debug `.app` 驗證 inactive／partial／active 三種狀態、取消、確認與部分失敗結果。

## 7. 完整驗證

- [x] 7.1 執行 locale 完整性 npm script、Board／layout／Skill 包 UI contract checks、`npm run build`、`npm run lint`、`cargo test --manifest-path src-tauri/Cargo.toml`、`spectra validate default-product-ui-to-traditional-chinese`、`spectra analyze default-product-ui-to-traditional-chinese --json` 與 `git diff --check`；要求所有命令 exit 0、Rust 測試 0 failed、Spectra 無 Critical／Warning，並以 bundle 產生的實體 debug `.app` 配合 Computer Use 完成繁體中文／English、舊 `zh` 相容轉換、light／dark／system、Skill 包混合成員與路由選取狀態、Skill 包加入／移除分離與移除確認、中央 Library／Project 未部署語意、四個拖曳目的地、Inspector checkbox、選取 lane 自動可見、失敗 rollback、Board／List、鍵盤操作、次要篩選器可見、長頁 sticky 不穿透與窄視窗水平捲動的人工矩陣。
