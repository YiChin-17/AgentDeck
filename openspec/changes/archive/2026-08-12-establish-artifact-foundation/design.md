## Context

Phase 2 已讓 Library 與 Project Board 以單一 Artifact identity 呈現 Skills，但 Rust／SQLite 仍只有 `SkillRecord` 與 `skill_targets`。`skills.id` 同時負責內容 identity、UI identity 與 deployment foreign key；Scenarios、Tags、sync metadata、Git object merge 和 CLI 也都直接使用 skill id。若 Phase 4–6 直接沿用這個結構，Plugin、Hook 與 Config Profile 的格式及部署規則會被迫塞進 Skill schema。

現有資料庫 schema version 是 7。`SkillStore` 持有唯一 SQLite connection；`SkillRecord` 與 `SkillTargetRecord` 也是 commands、scenario service 及 CLI 的穩定相容介面。Git backup 以 `.skills-manager/skills`、Scenarios、memberships、schema marker 與 merge protocol 2 為契約，deployment rows 不在 Git snapshot 內。外接 Library offline guard 位於 command／repo lock 邊界，SQLite 則固定留在內部 App state。

本 change 屬於 `plan.md` Phase 3。它只建立資料與型別基礎，不交付新的 Artifact 類型功能。

## Goals / Non-Goals

**Goals:**

- 建立與 subtype detail 分離的 Artifact identity，使 Skill、Plugin、Hook、Config Profile 可使用不同 detail schema。
- 建立同時表達 global／project scope 與 symlink／copy／CLI-managed mode 的 canonical deployment storage。
- 將 schema v7 的 Skills 與 global `skill_targets` 原值無損遷移到 schema v8，並以單一 SQLite transaction 保證失敗 rollback。
- 維持 `SkillRecord`、`SkillTargetRecord`、Scenarios、Tags、commands、CLI JSON、Board 與 sync engine 的 observable behavior。
- 固定 Phase 3 的 Git backup 相容邊界，避免在尚無第二種可備份 Artifact 時提前升級 protocol。

**Non-Goals:**

- 不建立 Hook、Plugin、Config Profile detail table 或 CRUD flow。
- 不新增 UI route、sidebar item、Board lane、Inspector 欄位或 frontend IPC shape。
- 不更改 Codex／Claude adapter、project scanner、symlink／copy 寫檔邏輯或 Plugin cache。
- 不修改 `.skills-manager` metadata layout、schema version、merge protocol、Git refs／trailers、Keychain service 或 CLI 名稱。
- 不支援 application downgrade；schema v8 database 由舊 binary 依既有 newer-schema guard 拒絕開啟。

## Decisions

### Artifact identity 與 typed detail 分離

新增 `src-tauri/src/core/artifact.rs`，定義：

- `ArtifactKind`：Rust enum 與 SQLite string 僅接受 `skill`、`plugin`、`hook`、`config_profile`。
- `ArtifactRecord`：只保存 `id` 與 `kind`，不把 subtype name、description、source metadata 或 secrets 提升為共用欄位。
- `ArtifactScope`：`Global` 或帶有 non-empty project id 的 `Project(String)`。
- `ArtifactDeploymentRecord`：保存 `id`、`artifact_id`、scope、agent、enabled、mode、source path、target path、last synced hash／time、status 與 last error。

SQLite 新增 `artifacts` parent table；`skills` 新增 unique `artifact_id`，既有 Skill 使用 `skills.id` 作為相同的 Artifact id。foreign key、unique index 與 triggers 共同阻止 missing parent、非 `skill` kind 或刪除 Skill 後留下 orphan identity。Plugin、Hook、Config Profile 將在各自後續 change 建立以 `artifact_id` 為 key 的 detail table。

未採用 single-table inheritance，因為它會產生大量 subtype-only nullable columns；未把 `artifact_type` 直接加到 `skills`，因為那仍讓 Skill table 成為所有類型的 parent。

### Canonical deployment storage 與 Skill compatibility API

新增 `artifact_deployments` 作為唯一 deployment source of truth，欄位如下：

- `id TEXT PRIMARY KEY`
- `artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE`
- `scope_type TEXT NOT NULL CHECK(scope_type IN ('global', 'project'))`
- `scope_id TEXT NOT NULL`，global 必須為空字串，project 必須為 non-empty id
- `agent TEXT NOT NULL`
- `enabled INTEGER NOT NULL CHECK(enabled IN (0, 1))`
- `mode TEXT NOT NULL CHECK(mode IN ('symlink', 'copy', 'cli-managed'))`
- `source_path TEXT NOT NULL`
- `target_path TEXT NOT NULL`
- `last_synced_hash TEXT`、`last_synced_at INTEGER`、`status TEXT NOT NULL`、`last_error TEXT`
- `UNIQUE(artifact_id, scope_type, scope_id, agent)`

`SkillStore` 繼續持有 SQLite connection，並新增 generic Artifact／deployment CRUD；不為單一 caller 引入第二個 connection wrapper。既有 `insert_target`、`get_targets_for_skill`、`get_all_targets` 與 `delete_target` 簽名維持不變，將 `SkillTargetRecord` 映射成 global、enabled deployment：`tool` 對應 `agent`，`source_hash` 對應 `last_synced_hash`，`synced_at` 對應 `last_synced_at`，source path 取同一 Artifact 的 `skills.central_path`。disabled 或 project-scoped rows 不會被舊 Skill target API 誤報為 global enabled target。

未保留 `skill_targets` 雙寫或 shadow table，因為兩份 canonical state 會在 crash 或 downgrade 後產生 silent drift。schema v8 完成後移除舊 table，舊 binary 由 user_version guard 明確拒絕。

### Schema v8 transaction migration 與明確 downgrade 邊界

`migrate_v7_to_v8` 在既有 migration transaction 內依序：

1. 建立 `artifacts` 與 `artifact_deployments`。
2. 以每筆 `skills.id` backfill 一筆 kind=`skill` 的 Artifact。
3. 在 `skills` 新增 nullable foreign-key column、以相同 id backfill、建立 unique index 與 invariant triggers；migration 結束後任何新寫入都不得產生 null 或 kind mismatch。
4. 將每筆 `skill_targets` 原 id 與欄位映射成 global、enabled deployment，並由 join 取得 source path。
5. 比對 Skill、Artifact、legacy target 與 deployment counts，執行 `PRAGMA foreign_key_check`；任一不一致即回傳 error。
6. 只有驗證成功才 drop `skill_targets` 並提交 user_version 8。

fresh database 仍從 v0 逐步升到 v8，以同一 migration path 建立最終 schema。migration 不讀 Library 檔案、不建立外接路徑，也不接觸 secrets。任何 SQL、constraint 或驗證錯誤會由既有 runner rollback，資料庫維持 v7；已成功升到 v8 後重跑為 no-op。舊 binary 對 user_version 8 顯示既有 newer-schema error，不嘗試自動 downgrade。

未採用啟動時 background backfill，因為 UI 與 deployment commands 可能在部分資料已轉換時開始讀寫；transaction migration 讓狀態只有完整 v7 或完整 v8。

### Legacy backup protocol 保持原格式

Phase 3 不改 `SCHEMA_VERSION`、`MERGE_PROTOCOL_VERSION`、`.skills-manager/skills/*.json`、Scenario／membership metadata 或 commit trailers。`sync_metadata` 仍序列化 `SkillMetaFile`，reindex 透過相容的 `upsert_skill` 自動建立／修復 kind=`skill` Artifact identity；deployment rows和現行 `skill_targets` 一樣不寫入 Git snapshot。

新增 round-trip 與 object-merge regression，證明同一組 v7 Skill／Scenario input 在升級前後產生 byte-identical metadata，pre-protocol restore 及 protocol 2 merge 行為不變。非 Skill Artifact 的 backup representation、cross-device merge 與 protocol bump 留給第一個需要備份該類型的後續 change。

未在 Phase 3 預先新增空的 Plugin／Hook metadata directory，因為目前沒有可驗證的內容 schema，舊 client 也無法安全合併它們。

### Offline、conflict 與 secret 邊界沿用既有 contract

SQLite migration 只操作內部 App state，因此 external Library offline 仍可啟動並完成 schema upgrade；migration 不嘗試同步或刪除 deployment target。migration 後，任何會寫入 Library 或實際 deployment path 的 command 仍必須先經過既有 `ensure_library_online`／library write lock；generic Artifact CRUD 若只改內部資料庫，不能被誤用來繞過 filesystem mutation guard。

`status` 與 `last_error` 只保存可顯示的狀態，不保存 token、credential、login payload 或完整 CLI environment。真實檔案與資料庫衝突仍沿用現有 sync／merge flow；Phase 3 不新增自動 conflict resolution。

## Implementation Contract

**Observable behavior**

- 升級既有 database 後，Library、Board、Agent Skills、Projects、Skill Packs、sync、backup 與 `skills-manager-cli` 顯示的 Skill ids、數量、targets、tags、memberships 及 JSON shape不變。
- fresh database 與升級 database 都是 user_version 8，且每筆 Skill 恰好對應一筆 kind=`skill` Artifact；每筆 legacy target 恰好對應一筆 global enabled deployment。
- Skill 新增、upsert、刪除及 target 新增／刪除在單一 store operation 後維持 parent、detail、deployment foreign-key integrity。
- migration failure 不留下 Artifact table 的部分資料、不刪 `skill_targets`，user_version 維持 7；再次以修正後資料啟動可成功重試。
- external Library offline 時 migration 可完成，但任何 Library／target filesystem mutation 仍回傳既有 `library_offline` error 且無副作用。

**Interfaces and data shape**

- `ArtifactKind` 的 persisted values 固定為 `skill`、`plugin`、`hook`、`config_profile`；未知值反序列化或寫入時回傳明確錯誤，不 fallback 成 Skill。
- Generic deployment uniqueness key 是 `(artifact_id, scope_type, scope_id, agent)`；global 使用空 `scope_id`，project 使用 non-empty project id。
- `SkillRecord` 與 frontend／CLI serialization 不新增 `artifact_id` 或 `kind` 欄位；其 `id` 同時是 Artifact id。
- `SkillTargetRecord` 的 Rust 與 serialized fields 維持現狀；相容 methods 只讀寫 global enabled deployments。
- `.skills-manager` metadata bytes、protocol marker與 commit trailer format 不變。

**Failure modes**

- kind、scope、mode、enabled constraint 違反、orphan foreign key、count mismatch 或 `foreign_key_check` finding 會中止 migration 並 rollback。
- project scope 缺少 project id、global scope帶 project id 或未知 deployment mode 會在 generic API 寫入前被拒絕。
- 未知未來 Artifact kind 不會被靜默忽略或當成 Skill；舊 binary 則由 schema version guard 拒絕整個 database。

**Acceptance criteria**

- migration tests 覆蓋 fresh、v7 populated、空 database、重跑、corrupt input rollback、newer schema rejection 與 foreign-key cascade。
- store tests覆蓋四種 `ArtifactKind` round-trip、兩種 scope、三種 mode、uniqueness、invalid values、Skill CRUD 相容與 target field-by-field mapping。
- metadata／merge tests 證明 upgrade 前後 canonical JSON bytes、protocol 2 marker、legacy restore與現有 object merge fixtures不變。
- focused tests、完整 Rust tests、frontend build、CLI build、i18n／Board／product identity checks及 `git diff --check` 全部 exit 0。

**Scope boundaries**

- In scope：Rust types、SQLite schema v8、migration、store compatibility methods與對應 tests。
- Out of scope：新的 Artifact UI／IPC、非 Skill detail schema、Plugin CLI、Hook／Config writer、backup protocol bump、公開 release migration。

## Risks / Trade-offs

- [Risk] `skills.artifact_id` 由 nullable column backfill，SQLite 無法直接替既有 table 加上 `NOT NULL` → 以 transaction backfill、unique index、foreign key與 INSERT／UPDATE triggers共同強制 invariant，並以 direct SQL negative tests驗證。
- [Risk] 移除 `skill_targets` 後任何漏改的 raw SQL 會在 runtime 失敗 → repository search assertion與完整 Rust tests要求所有 production queries經由新 table或相容 methods。
- [Risk] 舊 binary 無法開啟 schema v8 → 沿用現有明確 newer-schema rejection；不提供可能丟失 Artifact rows 的 downgrade。
- [Risk] future Plugin／Hook需求可能需要額外 deployment欄位 → subtype-specific data保留在未來 detail table；generic deployment只保存 plan.md已確認的跨類型欄位。
- [Risk] 保留 protocol 2代表非 Skill Artifact尚不能進入 Git backup → Phase 3明確不建立非 Skill資料；第一個需要跨裝置備份的後續 change必須升級並驗證 protocol。

## Migration Plan

1. 在 migration tests 建立包含 Skills、targets、Scenarios、memberships、Tags 與 conflicts 的真實 v7 fixture，先固定 row values及 metadata bytes。
2. 實作 schema v8與 negative rollback tests，確認 target backfill完成後才移除 legacy table。
3. 將 SkillStore CRUD與target compatibility methods切到 canonical Artifact tables，重跑既有 scenario、sync與command tests。
4. 驗證 offline startup、metadata round-trip、Git object merge、CLI JSON與Board contract。
5. 發佈後首次開啟由既有 migration runner自動升級；失敗時保留v7 database並回報初始化錯誤。

Rollback 僅指 migration transaction失敗時回到完整v7。成功升到v8後不自動降版；需要回復舊 binary時，使用者必須還原升級前的 database備份，舊 binary本身會拒絕v8避免 silent corruption。

## Open Questions

無。Phase 3的資料與相容邊界已由本 design固定；非 Skill backup protocol在對應功能 change再決定。
