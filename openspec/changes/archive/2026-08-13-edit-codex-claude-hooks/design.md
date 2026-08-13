## Context

Phase 4 第一個 change 已建立固定 Hook source descriptors、JSON／TOML Hook subtree parser、唯讀 Tauri DTO、Hooks route 與 compatibility registry。現況刻意不建立 Hook detail row，也不寫回外部設定；下一段必須在不接受任意 path、不執行 Hook、不洩漏內容到持久層的前提下，安全修改同一批來源。

這個 change 同時跨越 parser、filesystem transaction、SQLite identity、Tauri IPC 與 React UI。Hook 設定可能包含 command、prompt、URL、headers 或環境值；這些資料必須出現在當次 editor／preview 記憶體中，但不得進入 SQLite、一般 Library、logs、localStorage 或 `.skills-manager` Git backup。Codex TOML 需要保留註解與排列，Claude Code JSON 需要保留非 Hook sibling 與未知欄位。

專案以 macOS 為第一平台。實作仍需保持其他平台可編譯；runtime 若無法保證 atomic replacement，必須在寫入前 fail closed，不能降級成先刪後寫。

## Goals / Non-Goals

**Goals:**

- 只用既有 source id 與 optional linked Project id 解析可寫目標，不接受 frontend path。
- 用 Agent-specific operations 編輯 event、matcher、handler type 與已知 fields，並保留未修改的未知值與文件其他內容。
- preview 固定完整來源 revision、validation 與實際 Hook subtree diff；apply 遇到 stale revision 時拒絕。
- 每次 apply／restore 都先建立 recovery point，再以同目錄 staged file 與 atomic replacement 更新目標。
- 為第一次成功管理的 Hook source 建立 kind `hook` Artifact 與非敏感 detail／backup metadata。
- Hooks UI 只有在 preview 可套用且仍對應目前 draft 時啟用 Apply，所有 mutation 與 restore 都不可執行 Hook。

**Non-Goals:**

- 不執行或試跑 Hook，不提供 enable／disable。
- 不編輯 managed policy、Plugin-bundled、component 或 process-local Hook。
- 不跨 Agent 轉換 schema、不跨來源 merge、不批次套用。
- 不把 Hook payload 放入中央 Library、SQLite 或 Git backup，不升級 merge protocol 2。
- 不修改 Plugin、Config Profile 或既有 Skill deployment 行為。

## Decisions

### 固定 source capability 與可寫檔案邊界

`hook_management` 重用 `hook_inspection::source_descriptors`，Tauri commands 只接受 `projectId: string | null` 與 enum-backed `sourceId`。backend 先由 `SkillStore::get_project_by_id` 解析 Project，再從 descriptors 找完全相符的 source；unknown source、scope 不符或 unknown Project 分別回傳 `invalid_source` 或 `invalid_project`。frontend 永遠不提交 path。

寫入只允許 regular file，或位於既有 home／linked Project root 下的 missing fixed source。missing source可建立固定的 `.codex`／`.claude` parent；root 本身不存在或無法讀取時回傳 `source_offline`，不得建立 root。symlink、directory、device 與其他特殊檔案回傳 `unsupported_source_type`，避免 atomic replacement 改成替換 symlink 本身或跨出已核准邊界。

未採用任意 path picker，也不沿用 current working directory fallback，因為兩者都會破壞 inspection 已建立的讀取邊界。

### Patch operation DTO 與 Agent-specific validation

新增 `HookEditOperationDto`，支援 `create_handler`、`update_handler` 與 `delete_handler`。update／delete 使用原始 `event`、`groupIndex`、`handlerIndex` 定位；draft 帶新 event、matcher、handler type 與已知 field patches。backend 每次 preview／apply 都重新解析完整來源，再對當次 AST／TOML document 套用 operations，不接受 frontend 回傳的完整設定檔文字。

create 與 update 的 event、handler type、field name／value type 必須符合對應 Agent registry。既有 unknown event、handler type 與 fields 可原樣保留並可整筆刪除；unknown fields 不出現在可編輯 controls，也不能由 request 新增或改值。operation locator 找不到唯一節點時回傳 `stale_draft`。

JSON 操作保留 root 的所有非 Hook sibling keys 與未修改 Hook values，再序列化完整 document。TOML 使用既有 `toml_edit::DocumentMut` 對目標 table／array-of-tables 做局部修改，保留未修改 key、註解與排列。未採用把 canonical Hook JSON 整段覆寫 TOML 的做法，因為那會移除格式資訊與未知值。

### Preview revision 與精確衝突檢查

`preview_hook_change(projectId, sourceId, operations)` 回傳 `HookWritePreviewDto`：`sourceId`、`baseRevision`、`beforeCanonicalText`、`afterCanonicalText`、`validationIssues`、`canApply` 與 `wouldCreateFile`。`baseRevision` 是完整來源 bytes 的 SHA-256；missing source 使用固定字串 `missing`，不把原文放入 revision。

`apply_hook_change(projectId, sourceId, baseRevision, operations)` 在同一個 Hook write lock 內重新讀取來源、比對 revision、重跑相同 validation 與 transformation。bytes 或 missing／present 狀態不同即回傳 `source_conflict`，不建立 backup、不寫入、不更新 SQLite。Apply request 不接受 preview 產出的 after text，避免修改過的 payload 繞過 validation。

preview 只回傳 Hook subtree 的 before／after canonical text供現有 `DocumentDiffViewer` 使用；非 Hook siblings 不送到 frontend。超過既有 256 KiB／4,000-line diff gate 時 preview 回傳 `preview_too_large` 且 `canApply=false`，不把大型內容送入 O(n²) diff。

### Recovery backup 與 atomic replacement transaction

Recovery payload 放在 `central_repo::base_dir()/hook-backups/<artifact-id>/latest`，位於 application state，不在 `library_base_dir()/skills` Git tree。Unix directory mode 固定 0700、file mode 0600；無法建立私有權限時寫入前 fail closed。existing source 的 backup 是原始 bytes；missing source 的 backup 是 metadata 中的 absence marker，不建立空 payload。

apply 的順序固定為：取得 process-wide Hook write lock、重驗 revision與draft、建立 staged backup、寫入並同步同目錄 staged target、將 staged backup atomic promote 成 latest、atomic replace target、在 SQLite transaction 建立／更新 Artifact、Hook detail與backup metadata，最後移除 operation journal。journal 只含 ids、hashes、狀態與 backup locator，不含 Hook payload。若 SQLite commit 或後續步驟失敗，使用 latest backup補償回復原 bytes／absence；啟動時先處理未完成 journal，再開放 Hook mutation。

macOS／Unix 使用同目錄 rename 覆蓋保證 atomic replacement並保留原檔 mode。其他平台若 implementation 無法證明 equivalent replacement semantics，command 回傳 `atomic_replace_unsupported` 且不改檔；不使用 delete-then-rename fallback。

沿用現有 dependencies：`sha2` 產生 revision、`uuid` 產生 operation／backup ids、`serde_json` 與 `toml_edit` 修改文件、`rusqlite` 記錄非敏感 metadata。未加入新 crate。

### Hook Artifact identity 與 schema v9 metadata

schema v9 新增 `hook_details` 與 `hook_backups`。`hook_details` 以 `artifact_id` 連到 kind `hook` Artifact，保存 `source_id`、`context_key`、Agent、scope、format 與 timestamps；`context_key` 固定為 `global` 或 `project:<project-id>`，unique key 是 `source_id + context_key`。不保存 source path，因為 path 每次都由 fixed descriptor 與 Project record解析。

`hook_backups` 只保存 backup id、artifact id、before／after SHA-256、backup kind `bytes|absent`、state-relative locator、created time 與 restore time。表格與 error columns不得含 Hook payload、command、prompt、URL、headers 或 environment。Artifact 與 detail 只在第一次 apply／restore 成功時建立；preview 不改 SQLite。

v8→v9 migration 在一個 SQLite transaction 中建立 tables、indexes 與 kind constraints，既有 rows不變且不建立 seed Hook artifacts。fresh database與 upgraded database schema必須一致，migration failure保持 user_version 8。舊 binary遇到 v9沿用既有 newer-schema fail-closed 行為。

未建立 Artifact deployment row：本 change 管理的是 Agent 已讀取的固定 config source，不是由中央 Library部署的副本。未修改 sync metadata、Git refs、trailers或 protocol marker。

### Conflict-safe restore 與單一 recovery point

`get_hook_recovery(projectId, sourceId)` 只回傳 latest backup metadata 與目前是否仍符合可 restore revision；不回傳 backup payload。`preview_hook_restore(projectId, sourceId, backupId)` 回傳目前 Hook subtree與 backup Hook subtree的 canonical diff、`baseRevision` 與 `canApply`。`apply_hook_restore` 必須帶同一 base revision；目前來源改變、backup id過期或 backup不可讀時 fail closed。

restore 寫入前先把目前來源建立成新的 recovery point，再 atomic restore 舊 bytes；absence backup 則 atomic移除目前 regular file。成功後新的 recovery point成為 latest，因此使用者可再 restore 回復 restore 前狀態。restore 不會改動其他 source，也不會執行 Hook。

只保留每個 Artifact 一個 latest recovery payload與其 active metadata，避免無界保存可能含敏感資訊的歷史。替換舊 backup必須在新 target transaction成功後進行；失敗時保留先前可用 recovery point並回報錯誤。

### Hooks UI 的 draft、preview 與 apply 狀態機

`HookEditor` 從選定 entry或 valid／missing source開始，顯示 Agent-specific fields與 existing unknown read-only rows。狀態固定為 `editing → previewing → preview_ready → applying → applied`；任何 draft change 都使既有 preview失效並停用 Apply。validation errors逐欄顯示，backend errors以 typed localized message顯示。

Edit、Delete、Preview、Apply與Restore controls只對可寫 regular／missing fixed source顯示；invalid、too_large、symlink、offline source維持 inspection與diagnostic但不可 mutation。Apply不可由一次 click同時 preview並寫入。頁面保留既有 filters、Inspector、source comparison與matrix，且仍不提供 Execute／Test control。

不把 draft、Hook content、preview或backup payload放入 `AppContext` 或 localStorage；route-local memory在 project selection改變或 apply完成後清除。沿用 latest-request-wins guard，並在 mutation進行時固定 source／Project context。

## Implementation Contract

**Observable behavior**

- 使用者可在 valid source編輯或刪除 handler，也可在 existing root的 missing fixed source建立第一個 handler；invalid、too-large、offline或symlink source不可寫。
- Preview顯示 backend對完整原文件套用 operations後的 Hook subtree差異；非 Hook sibling不會出現在 preview。
- Apply只能使用未過期 preview所對應的 base revision與draft；外部修改會顯示 conflict且保持檔案、backup與database不變。
- 成功 apply後重新 inspection會顯示新 Hook內容；TOML未修改註解／排列與 JSON非 Hook siblings仍存在。
- 每次成功 apply／restore都有一個 owner-private latest recovery point；restore需先 preview，且可再 restore回復 restore前狀態。
- 所有流程都不執行 Hook，也不把 payload寫入 SQLite、Library、logs、localStorage或 Git backup。

**Interface and data shape**

- Tauri commands：`preview_hook_change`、`apply_hook_change`、`get_hook_recovery`、`preview_hook_restore`、`apply_hook_restore`；每個 command只有 `projectId`、`sourceId` 與相應 typed request，不含 filesystem path。
- edit operation types固定為 `create_handler`、`update_handler`、`delete_handler`；locator使用 event、group index、handler index。
- write errors至少固定為 `invalid_project`、`invalid_source`、`source_offline`、`unsupported_source_type`、`invalid_hook_draft`、`stale_draft`、`source_conflict`、`preview_too_large`、`backup_failed`、`atomic_replace_unsupported`、`write_failed` 與 `restore_failed`。
- persisted Hook Artifact kind固定為 `hook`；detail unique key固定為 source id與 context key；backup metadata不得含 Hook payload。

**Failure modes**

- validation、revision、backup、permission、fsync、rename或SQLite任一步驟失敗時，command回傳 typed sanitized error，不留下 partial target；需要補償時從 latest backup回復。
- startup發現 operation journal時，在提供 mutation commands前回復或完成一致狀態；journal處理失敗時所有 Hook mutation回傳 `recovery_required`，inspection仍可用。
- Project root offline不得建立替代目錄；external Library offline不影響 user config inspection，但不得導致 Hook backup被寫入 external Library。
- backup payload不可讀或hash不符時 restore fail closed，不刪除目前 source。

**Acceptance criteria**

- Rust tests固定 JSON與TOML create／update／delete、unknown preservation、comment／order preservation、missing creation、invalid draft、locator mismatch、hash conflict、symlink／offline refusal、256 KiB／4,000-line preview gates。
- fault-injection tests逐點覆蓋 backup write、target staged write、atomic replace與SQLite commit failure，斷言 target bytes／absence、backup metadata與Artifact rows一致。
- migration tests證明 populated v8無損升級到v9、fresh／upgraded schema一致、failure rollback與idempotence。
- security fixtures在Hook payload放入 `sentinel-secret`，斷言SQLite dump、Library tree、Git status、logs、localStorage contract與serialized errors不含該值；只有source、當次IPC preview與private recovery payload可含。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:hooks-ui`與`git diff --check`全部exit 0；temporary HOME／linked Project手動完成preview、external edit conflict、apply與restore。

**Scope boundaries**

- In scope：固定 user／linked-project sources、單一 source operations、Agent-specific validation、preview、conflict detection、private latest backup、atomic apply／restore、Hook identity metadata與Hooks UI controls。
- Out of scope：Hook execution、CLI validation、cross-Agent conversion、multi-source transaction、managed／Plugin／component Hooks、Library/Git payload backup、Plugin與Config Profile功能。

## Risks / Trade-offs

- [Risk] Filesystem與SQLite無法形成單一原生transaction → 以private backup、operation journal、補償回復與startup recovery維持可恢復的一致狀態。
- [Risk] Hook payload可能含secret而recovery需要保存原 bytes → backup限application state、owner-private權限、單一latest版本，且不進database、Library、log或Git。
- [Risk] TOML局部編輯可能破壞comments或unknown layout → 使用 `toml_edit` node-level operations與byte fixture round-trip tests，不以canonical JSON替換整個table。
- [Risk] index locator在外部修改後指到另一筆handler → full-source revision先阻擋，locator仍找不到唯一節點時回傳 `stale_draft`。
- [Risk] 不同平台replace semantics不一致 → 只在能保證atomic replace的平台啟用，其他平台寫入前回傳明確unsupported error。
- [Risk] 單一latest backup限制歷史回復 → restore本身建立reverse recovery point，滿足一次undo，同時避免無界累積敏感payload。

## Migration Plan

1. 先以failing tests固定operations、validation、round-trip、revision、backup／atomic failure與migration契約。
2. 新增schema v9 tables與store APIs，驗證fresh／upgrade／rollback後才接filesystem mutation。
3. 實作preview與apply／restore core，再註冊Tauri commands與typed DTO。
4. 接上HookEditor狀態機、雙語訊息與static UI contract，最後跑完整Rust／frontend／manual fixtures。
5. Rollback程式碼前先確認沒有pending journal；schema v9保留Hook metadata tables，舊binary依newer-schema guard拒絕開啟，不能直接降版。

## Open Questions

無。
