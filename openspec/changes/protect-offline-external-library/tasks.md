## 1. 儲存邊界與相容遷移

- [x] 1.1 依照「固定內部 App state，外部設定只決定 Library content root」先為「Application state remains available independently of an external Library」新增 failing Rust tests，再調整 `central_repo`／`app_state` 路徑解析，使 SQLite、scenarios、cache、logs 固定使用 internal state，而 default internal Library 行為不變；以 focused tests 斷言 external root 離線仍能開啟 internal DB、configured path 不被建立，default install root維持既有值。
- [x] 1.2 為「Legacy external repository configuration migrates without data loss」的 online 路徑先新增 failing migration tests，再實作 versioned config與 copy-and-verify：驗證 state 後才切換 external `skills` root，legacy來源不刪除；以 temporary source／internal target 斷言 DB 可開啟、scenario rows與 Skill row counts一致、config marker只在驗證後完成。
- [x] 1.3 完成同一 migration requirement的 offline與 conflict分支：legacy volume缺失時保留 retry marker，兩端非等價 state時回報 `migration_blocked`且不 blind merge；以 focused Rust tests比較前後目錄 hash、config內容與 SQLite row counts，確認兩端都沒有被覆寫或刪除。

## 2. Availability、identity 與 Retry

- [x] 2.1 依照「用持久 Library identity 與無副作用 probe 判斷 online」為「Configured Library availability is verified without side effects」先新增 missing、unreadable、not-writable、identity-mismatch與valid cases，再實作 Library marker及 probe；以 temporary roots 斷言 reason code、probe前後directory hash與不存在路徑仍不存在。
- [x] 2.2 依照「LibraryAvailability 是所有 flow 共用的 runtime contract」建立 thread-safe availability state、`LibraryAvailabilityDto`、`get_library_availability`、`ErrorKind::LibraryOffline`與 frontend error mapping；以 Rust serialization／direct command tests及`npm run build`確認 DTO shape、stable reason與`library_offline`錯誤不由 client path推導。
- [x] 2.3 依照「Retry 只在原 Library 驗證成功後恢復服務」完成「Reconnect restores service only after full verification」：`retry_library_availability`僅在 identity、metadata refresh與watcher restart全部成功後切 online，否則維持 offline且不 replay mutation；以 focused tests覆蓋成功、wrong identity、refresh failure與watcher failure的 state transition及no-mutation assertions。

## 3. Fail-closed backend 與背景流程

- [x] 3.1 為「Offline state fails closed across Library operations」先加入 installer、import／reimport、update與delete的 direct function／IPC failing tests，再讓下層入口共用 `ensure_library_online()`；以 filesystem hash、Skill rows、target rows與audit/Git state前後比較證明 offline回傳`library_offline`且沒有副作用，online regression維持通過。
- [x] 3.2 將 scenario／Preset、Agent Skills與Project deployment sync接到同一 guard，避免UI以外的direct IPC在offline時改動targets；以 `scenario_service`、`agent_workspace`、`projects`、`presets` focused tests驗證 offline拒絕與online原行為，並斷言target paths與database memberships不變。
- [x] 3.3 依照「Offline 轉換不產生刪除或同步副作用」讓 startup metadata reindex／scenario apply、file watcher、auto backup、manual Git backup在offline時跳過或停止，runtime filesystem error會切offline且中止後續步驟；以 app_state、sync_metadata、file_watcher、auto_backup與git_backup focused tests驗證missing Library不被解讀成delete、沒有commit／metadata write且reconnect後可重新啟動。

## 4. 全域 UI 與可操作性

- [x] 4.1 在 `src/lib/tauri.ts`與`AppContext`載入 availability DTO並提供 Retry／refresh，完成「Offline state is visible throughout the product」的frontend state contract；以`npm run build`、`npm run lint`及mocked command source review確認loading／error／online／offline狀態不以路徑字串推導。
- [x] 4.2 在`Layout`共用內容容器建立`LibraryOfflineBanner`（board route與一般route皆可見）、Settings path／reason／Retry顯示與`en`／`zh-TW`雙語文案，並讓`MySkills`的install／import／reimport／update／delete、`ProjectDetail`的deployment sync、`PresetBar`的Skill包add／remove、`ArtifactBoard`的lane拖曳（`onMoveToLane`）與Backup controls在offline時disabled，Settings／diagnostics保留；以`npm run check:i18n`驗證雙語key與placeholder parity及台灣術語表、以`npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui`確認未破壞既有board與Skill包來源字串斷言，並以Computer Use逐頁驗證banner、configured path、disabled actions（含拖曳不觸發mutation）、cached inventory標記與Library文件offline錯誤。
- [x] 4.2a 補上 Computer Use 驗證揭露的 offline 可見性缺口：新增 `poll_library_availability`（online 才 re-probe，offline 只能由 Retry 解除）並在 `Layout` 每次進入頁面時呼叫，讓拔碟後的 offline 狀態不必等到下一次 mutation 才浮現；把 `WorkspaceView`（Agent Skills）、`Dashboard` 匯入入口與 `MultiSelectToolbar` 批次寫入操作接上 `libraryOffline`；在 `MySkills` 標題顯示 `library.offline.cachedNotice`；`ArtifactBoard` 於 `mutationsDisabled` 時關閉 grab cursor 與文字選取；`get_skill_document` 補上 `ensure_library_online()` 並由 `SkillDetailPanel` 依 `library_offline` error kind 顯示新增的 `library.offline.documentUnavailable`，離線開啟 Library 文件不再誤報成「找不到文件」。以 `poll_takes_a_vanished_library_offline`、`poll_never_restores_an_offline_library`、`poll_leaves_a_healthy_library_online_without_writing`、`opening_a_document_while_offline_reports_offline_not_a_missing_file` 四支 focused tests 與實機拔碟後切頁、開啟文件流程驗收。
- [x] 4.3 以temporary external Library實機執行online→移除root→offline→恢復原root→Retry流程，確認錯誤volume identity不能恢復、正確identity可恢復watcher與actions且沒有queued writes；以Computer Use畫面狀態、backend logs與前後filesystem／SQLite snapshot作為驗收證據。
- [x] 4.3a 修掉同一流程揭露的兩個 migration 缺陷：`set_base_dir_override` 換 root 時清掉屬於舊 root 的 `library_id`（否則新 root 尚無 marker 而被判 `identity_mismatch`，重啟後直接離線）、`directory_has_entries` 不把 availability probe 自己寫的 Library marker 當成使用者資料（否則遷移永遠 blocked），並讓遷移完成後 `library_id` 對齊隨資料搬移過去的 marker；以 `moving_the_library_drops_the_identity_of_the_old_root`、`re_selecting_the_same_root_keeps_its_identity`、`a_root_holding_only_the_library_marker_still_counts_as_empty`、`migration_adopts_the_identity_that_moved_with_the_library` 四支 focused tests 與實機「設定外接庫→重啟→直接 online」驗收。

## 5. 完整驗證

- [x] 5.1 執行所有新增focused Rust tests、`cargo test --manifest-path src-tauri/Cargo.toml`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui`、`spectra validate protect-offline-external-library`、`spectra analyze protect-offline-external-library --json`與`git diff --check`；要求tests/build/lint/四支check腳本exit 0、Rust 0 failed、Spectra無Critical／Warning，並以`git diff`確認未修改database schema、Git backup protocol、CLI explicit root語意、Agent／Project routing、board lane定義／Inspector版面或`src/i18n`的語系組成。
