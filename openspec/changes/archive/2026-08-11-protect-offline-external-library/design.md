## Context

`plan.md` 要求 SQLite 等 App state 留在內部磁碟，中央 Library 則可由使用者改到外接磁碟；外接來源離線時必須顯示 `Library Offline` 並停止同步與刪除。現有 `central_repo::base_dir()` 同時決定 `skills/`、SQLite、scenarios、cache 與 logs，設定外部 central repo 等於把所有 state 一起移走。`ensure_central_repo()` 又會對解析出的目錄執行 `create_dir_all`；macOS volume 未掛載時，原本 `/Volumes/<name>` 路徑可能被建立成內部磁碟上的空資料夾，造成空 Library 與新 DB 啟動。

上游已有安全的 pending migration、config corruption warning、path normalization 與 `external_base_dir()` 模式，可作為相容基礎；但 Tauri App 尚未有 Library availability state，也沒有跨 install、sync、delete、metadata、watcher 與 backup 的統一 guard。

已歸檔的 `default-product-ui-to-traditional-chinese` 改變了本 change 要接上的 frontend 現況：語系從三個縮成 `en` 與 `zh-TW`（`zh.json` 已刪除，legacy `zh` 在 `i18n/index.ts` 正規化為 `zh-TW`）；Library 與 Project 改為四欄 Artifact Board 加 docked Inspector，`ArtifactBoard` 的 lane 拖曳會經 `onMoveToLane` 觸發部署 target mutation；`PresetBar` 的 Skill 包改為分離的 add／remove 動作；`Layout` 已在 board 與非 board 兩種容器內渲染共用 `StatusBanner`。專案同時新增四支以來源字串比對的檢查腳本（`check:i18n`、`check:board`、`check:board-layout`、`check:skill-pack-ui`），改動這些元件時必須一併維持通過。

## Goals / Non-Goals

**Goals:**

- 將固定內部 App state 與可配置 Library content root 分離，保持 SQLite 在可啟動的位置。
- 在不建立目錄的前提下判斷已設定 Library 是否為原本那個可讀寫來源。
- App 離線時仍可啟動、顯示狀態與最後已知 inventory，但任何可能寫入 Library 或部署目標的流程 fail closed。
- 讓使用者以 Retry 明確恢復 online 狀態，重新掛接 watcher 與資料 refresh。
- 安全承接舊 `repo_path` layout；無法判定或 migration 發生衝突時保留來源且不盲目合併。

**Non-Goals:**

- 不自動 mount、搜尋改名 volume、fallback 到預設 Library 或建立離線寫入 queue。
- 不在 offline 時提供 Skill 文件內容的快取副本，也不假裝檔案仍在線。
- 不改 database schema、Git backup protocol、Agent／Project target routing 或 sync mode。
- 不改 CLI 明確傳入的 `--skills-root`／`--path` 語意；CLI caller 仍為自己指定的 root 負責。
- 不開始 Phase 2 Board、Plugin、Hook 或 Config Profile 工作。

## Decisions

### 固定內部 App state，外部設定只決定 Library content root

Tauri App 的 SQLite、scenarios、cache 與 logs 固定解析到內部 `default_base_dir()`；Library content root 由預設 `<internal-base>/skills` 或 config 指定的外部 Library 決定。既有 `central_repo::skills_dir()` 繼續作為 Library root 的單一入口，避免下游自行拼接設定路徑。

config 使用版本化欄位保存 external Library base、其 `skills` root 與 migration 狀態。舊 `repo_path` 視為 legacy base：在線時先驗證 layout，再把必要 App state 複製到內部 target 並保留來源供 rollback，最後把 Library 指向 legacy base 的 `skills`；離線時只記錄 `migration_pending_offline`，不得建立外部路徑或空 Library。internal target 非空且內容不相容時進入 `migration_blocked`，不 blind merge。

替代方案是繼續讓 DB 位於外部來源，offline 時直接讓 App 啟動失敗；這無法顯示 `Library Offline` 或保留安全操作介面，因此不採用。

### 用持久 Library identity 與無副作用 probe 判斷 online

新建或成功採用的 Library root 寫入 AgentDeck-owned identity marker，包含隨機穩定 ID；config 保存預期 ID。啟動、Retry 與每個 mutation 前的 probe 只使用 metadata/read/write capability 檢查既有 root 與 marker，不呼叫 `create_dir_all`。缺失、不可讀、不可寫或 marker 不符分別回報穩定 reason code。

legacy Library 首次在線採用時可在確認既有 `skills` layout 後建立 marker；如果 legacy path 離線，必須等使用者重新接上後才能採用。marker 不符視為可能掛載到另一個 volume 或替代資料夾，必須 fail closed，不能只因路徑字串相同就 online。

替代方案是只檢查 `Path::exists()`，但空掛載點或不同 volume 會得到錯誤的 online 結果，因此不採用。

### LibraryAvailability 是所有 flow 共用的 runtime contract

建立 thread-safe runtime availability state，至少包含 `state`、`reason`、`configured_path`、`library_id`。`get_library_availability` 回傳 DTO；`retry_library_availability` 重新 probe，成功才切到 online 並觸發 metadata refresh 與 watcher restart。

所有會寫入 Library 或部署目標的下層入口先呼叫 `ensure_library_online()`，offline 時回傳 dedicated `library_offline` AppError；command/UI 的禁用只是體驗層，不能取代 backend guard。包含 installer、sync／scenario、delete、metadata write/reindex、Git backup 與 agent/project actions。文件讀取若需要 Library 檔案也回傳相同錯誤；純設定、診斷與 cached DB inventory 可讀。

替代方案是在每個 frontend button 個別判斷，direct IPC、background job 與未覆蓋的 flow 仍會寫入，因此不採用。

### Offline 轉換不產生刪除或同步副作用

startup probe offline 時跳過 metadata reindex、startup scenario apply、file watcher 與 auto backup；runtime marker 消失或 probe 失敗時立即把 state 切為 offline，停止 watcher/background round。任何已取得舊 path 的操作在實際 mutation 前再 probe，縮小斷線 race；失敗不刪 DB row、target row、central path 或 on-agent target。

cached inventory 可以顯示，但 UI 以全域 banner 與 disabled actions 明確標示資料只代表最後已知狀態。Settings 顯示 configured path、reason 與 Retry；不提供「使用預設 Library 繼續」捷徑。

### Retry 只在原 Library 驗證成功後恢復服務

Retry 成功條件為原 configured root 可讀寫且 identity 符合。成功後依序 refresh metadata、重新啟動 watcher/background scheduling、刷新 frontend data；任一步失敗維持 offline 並回傳具體 reason，不留下部分 online 狀態。Retry 不執行 queued writes，因本 change 沒有離線 queue。

## Implementation Contract

- **Runtime DTO:** `LibraryAvailabilityDto` 至少提供 `state: "online" | "offline"`、穩定 `reason` code、`configured_path` 與 nullable `library_id`。frontend 不自行以路徑存在性推導狀態。
- **Error contract:** 受保護 backend operation 在 offline 時回傳 `ErrorKind::LibraryOffline`；錯誤不伴隨 filesystem、SQLite target、metadata 或 Git mutation。frontend 將它映射為本地化訊息，不顯示一般 unknown error。
- **Startup contract:** configured external Library 不可用時，App 使用內部 state DB 啟動，回傳 offline DTO，跳過 Library reindex、startup sync、watcher 與 backup；不得建立 configured path、預設替代 Library 或新的 external DB。
- **Online contract:** default internal Library 與已驗證 external Library 的既有 install、update、delete、sync、Git backup 與 watcher 行為維持不變。
- **Migration contract:** old `repo_path` 在線且 target state 可安全採用時，複製必要 state、驗證後更新 versioned config，來源保留供 rollback；來源離線或兩端衝突時不 blind merge、不清 marker，App 顯示 offline 或 migration-blocked reason。
- **Reconnect contract:** `retry_library_availability` 只有在 original root 與 identity 驗證成功、refresh/watcher restart 均成功後切 online；失敗保持 offline。
- **UI contract:** `LibraryOfflineBanner` 由 `Layout` 的共用內容容器渲染（與既有 `appError` 的 `StatusBanner` 同層），因此 board route（`/my-skills`、`/project/*`）與一般 route 都可見；banner 顯示 configured path、reason 與 Retry。offline 時 disabled 的控制項至少包含：`MySkills` 的 install／import／reimport／update／delete、`ProjectDetail` 的 deployment sync、`PresetBar` 的 Skill 包 add／remove、`ArtifactBoard` 的 lane 拖曳（`onMoveToLane`）與 backup actions；Settings／diagnostics 仍可用。
- **Existing-UI contract:** 本 change 不改 board lane 定義、Inspector 欄位或 Skill 包互動語意，只加入 disabled 條件；`npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui` 必須維持通過。這三支腳本以來源字串比對斷言（例如 `check-skill-pack-ui.mjs` 要求 `PresetBar.tsx` 保留 `t("presetBar.add")`、`setPendingRemoval({ preset, count: s.installed })`、`onConfirm={() => handleDeactivate(pendingRemoval.preset)}` 等片段），改寫這些元件時必須保留原有片段而非重構掉。
- **Localization contract:** 新增文案只寫入 `src/i18n/en.json` 與 `src/i18n/zh-TW.json`；`en` 為 baseline，兩邊 leaf key 與 interpolation placeholder 必須一致，zh-TW 必須符合 `check-i18n-locales.mjs` 的台灣術語表（例如用「本機」「快取」「重新整理」「唯讀」，不得用「本地」「緩存」「刷新」「只讀」）。`npm run check:i18n` 為此項的驗收指令。
- **Acceptance criteria:** Rust tests覆蓋 missing mountpoint 不建立、unreadable/read-only/identity mismatch、legacy online/offline migration、runtime disconnect、guard no-mutation、Retry success/failure 與 internal Library regression；frontend `npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui` 全部通過，Computer Use 人工驗證 offline banner、disabled actions 與 reconnect。
- **In scope:** config-driven Tauri App 的 Library root、internal state、availability DTO/error、guards、watcher/background pause、`Layout` global banner、既有 Library／Project／Skill 包／Board 控制項的 offline disabled 條件、Settings Retry 與 en／zh-TW 雙語文案。
- **Out of scope:** CLI explicit roots、OS automount、offline queue、DB schema、protocol version、board lane 定義與 Inspector 版面調整、新增語系檔或其他 artifact 類型。

## Risks / Trade-offs

- [Risk] legacy external base 同時包含 DB 與 Library，migration 中斷可能形成兩份 state → 使用 versioned pending marker、copy-and-verify、不刪來源；只有驗證後切換 internal state。
- [Risk] 路徑存在但實際是錯誤 volume → identity marker 必須匹配；未知 marker 不自動採用。
- [Risk] 每次 mutation probe 增加少量 I/O → probe 只讀 root／marker metadata，成本遠小於錯誤寫入風險。
- [Risk] backend flow 遺漏 guard → 將 guard 放在 installer、sync、metadata、backup 等下層入口，並以 direct function／IPC tests 覆蓋，不只靠 UI。
- [Risk] runtime 拔除發生在 probe 後、write 前 → 下層 filesystem error同樣轉為 offline 並停止後續步驟；不能完全消除 OS race，但不得把錯誤轉成 delete/reconcile。
- [Risk] cached DB inventory與磁碟實況不同 → UI 明示 last-known offline state，文件與 mutation不可用，reconnect 後才 refresh。

## Migration Plan

1. 讀取舊 config；default internal layout 保持原位，不執行 migration。
2. 對 configured legacy base 做無副作用 probe；offline 時記錄 pending state並啟動 offline UI。
3. online 時 copy 必要 DB／scenario state 到內部 staging，驗證 SQLite 可開啟與必要 row counts，再原子更新 config。
4. 建立／驗證 Library identity marker並讓 `skills_dir()` 指向 external `skills` root；保留 legacy state files，不在本 change刪除。
5. rollback 讀取 versioned config的 legacy source資訊，恢復舊 base解析；因來源未刪除，不需要從 backup重建。

## Open Questions

無；Phase 1要求的 offline fail-closed、internal state與 explicit Retry 已定案，legacy來源採 copy-and-verify且不刪除。
