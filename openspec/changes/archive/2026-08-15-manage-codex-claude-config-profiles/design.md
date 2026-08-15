## Context

`inspect-codex-claude-config-profiles` 已提供固定來源 discovery、1 MiB bounded parse、exact allowlist、typed DTO、source fingerprint、supported-source precedence 與唯讀 UI。Phase 6 的剩餘目標是讓同一組非敏感設定可重複套用到已登錄專案，且在外部修改、格式錯誤、Library offline 或任一持久化步驟失敗時都不覆蓋來源或留下部分資料。

現有 Artifact foundation 已支援 `config_profile` kind 與 canonical deployment rows；Hook management 已有 fixed target、preview token、same-directory atomic replace、owner-private recovery point 與 fault injection 的可重用模式。Config Profile 仍有不同資料語意：一個 profile 同時持有 Codex／Claude Code 的 typed entries，一個 Artifact 可對多個 Project／Agent assignment，而 source transformation 必須只修改 allowlist keys 並保留未知內容。

利害關係人是維護多個 Codex／Claude Code 專案設定的使用者，以及後續實作／審查 profile mutation 的開發者。限制是不得新增 production dependency、不得把 secret 或 raw source 帶入 Library／SQLite／一般 DTO／log、不得從 frontend 接受 path，也不得破壞上游跨平台行為。

## Goals / Non-Goals

**Goals:**

- 持久化只含 exact allowlist typed scalar 的 ConfigProfile Artifact detail，並以 canonical deployment row 表示 Project／Agent assignment。
- 對固定 Codex／Claude project sources 產生可審查、可失效的 typed preview。
- apply 時重新驗證 profile revision 與 source fingerprint，只修改 selected allowlist entries並保留未知內容。
- 以 owner-private recovery point、same-directory staged file、sync 與 atomic replace 提供 rollback 與 restore。
- 在 Config Profiles 頁面提供明確分離的 inspection 與 management flows，取消或錯誤時零 mutation。

**Non-Goals:**

- 不修改 user sources、Claude project-local source、managed policy、CLI flags 或 environment resolution。
- 不保存或引用 secret，不接觸系統安全儲存，不接受 raw document、任意 key、path 或 caller-controlled command。
- 不做 background auto-apply、watcher write、跨專案單鍵批次 apply、profile import/export 或 Git backup payload。
- 不修改 Skill、Plugin、Hook 的 detail、deployment 或 recovery semantics。
- 不在此 change 重構 Hook writer 成通用 abstraction；只有第二個實際 caller 出現且共享 contract 完全相同時才抽取最小共用 helper。

## Decisions

### Typed ConfigProfile persistence 與 canonical assignment integrity

新增 `config_profile_details` 與 `config_profile_entries`。detail 以 `artifact_id` 一對一連到既有 `artifacts(kind = 'config_profile')`，保存 revision 與 timestamps；entry 的唯一鍵為 profile／Agent／canonical key，並以 `value_type` 加互斥 scalar columns 表達 string、boolean、integer，SQL CHECK 保證恰有一個 typed value。service 在 transaction 內再次用 inspection allowlist 驗證 Agent、canonical key 與 type，避免 frontend 或舊資料插入未知／敏感 shape。

Project assignment 不建立第二套 assignment source of truth；沿用 canonical `deployments`，scope 固定為 Project、Agent 固定 Codex 或 Claude Code、mode 固定為 Config Profile 專用的 managed write mode，source identity 為 profile Artifact，target identity 由 backend fixed source enum 衍生。create／edit／assignment mutation 在 SQLite transaction 中更新 Artifact、detail、entries 與 deployments；revision 每次 entry 變更遞增。刪除只允許沒有 deployment 與 recovery metadata 的 profile。

替代方案是將整份 profile 存成 JSON blob或另建 assignments table。JSON blob 會弱化 type／key constraint；第二套 assignment table 會與 canonical deployment drift，因此拒絕。

### Fixed project target authority 與 explicit scope exclusion

所有 management request 只接受 profile ID、registered Project ID、Agent 與 typed entry／operation。backend 透過 `SkillStore` 取得 project root，Codex target 固定為 `<root>/.codex/config.toml`，Claude Code target 固定為 `<root>/.claude/settings.json`。unknown project、symlink、special file、unavailable root、source over 1 MiB 或 invalid document 在 preview 前拒絕；missing regular target 可 preview 為 create。

不接受 scope/path/home/cwd/environment。user sources 與 `.claude/settings.local.json` 沒有 writable target enum，從型別層級排除。

替代方案是重用 inventory 的 arbitrary scope filter決定 write target；filter 是顯示選項，不足以作為 mutation authority，因此拒絕。

### Revision-bound typed preview token

`preview_config_profile_apply` 讀取 profile snapshot與 fixed target snapshot，產生 allowlisted typed diff，狀態固定為 `same`、`added`、`changed`、`removed`。Profile 不含的 key維持來源現況，不以 absence 表示刪除；只有 entry 中顯式存在的 key可新增或取代。preview token 在記憶體中綁定 profile ID／revision、Project ID、Agent、target source ID、base fingerprint或 absent marker、exact transformed bytes hash與 typed diff，設 expiry且只能 consume 一次。

`apply_config_profile` request 只含 token。apply 在 global Config Profile write lock內 consume token、重讀 profile／Project／source並重新 transform；任何 identity、revision、fingerprint、target kind或 output hash不符回 `stale_preview`，且不建立 backup、不寫檔、不改 deployment status。

替代方案是把 diff或 desired entries隨 apply request再送一次；那會讓 apply authority超過 preview，因此拒絕。

### Agent-specific preservation transform

Codex 使用 `toml_edit::DocumentMut`，只對 exact top-level allowlist keys設定與 profile scalar type相符的 value；Claude Code 使用 `serde_json::Map`，只更新 exact top-level keys與 `permissions.defaultMode` leaf。其他 key/value、nested sibling與 TOML comments／ordering保留。invalid source不自動 repair或替換；missing target由最小合法 document建立。

response只回 typed diff、stable codes與 fixed source ID，不回 transformed raw document、backup bytes、unknown key name/value或 parser／OS error。寫入後重新 parse並驗證 selected entries與 fingerprint。

替代方案是 redacted whole-document preview或 serialize一份全新 document；前者仍可能洩漏未知內容，後者會刪除未知欄位與註解，因此拒絕。

### Atomic apply、deployment state 與 failure rollback

寫入前將原始 bytes或 absent marker保存到 app-internal owner-private recovery file，SQLite只保存 recovery ID、Artifact／Project／Agent／source ID、before／after fingerprint、kind與相對 storage key，不保存 bytes或 path。staged target建立在同一目錄，以 owner-only permissions寫入並 sync file；支援的平台以 atomic rename取代，再 sync parent directory。

成功 replace後，在 transaction內 upsert canonical deployment的 target identity、last synced fingerprint／time與 clean status並 promote recovery metadata。若 staged write、sync、replace、post-write verification或 SQLite commit失敗，流程以 recovery bytes／absence把 target原子回復；回復也失敗時回 `rollback_failed` 並保留 recovery point供人工 restore，不宣稱 deployment成功。缺少可保證 atomic replace的平台在 mutation前回 `atomic_replace_unsupported`。

Library offline時 profile CRUD與assignment依賴 SQLite／Library policy而被既有 write gate拒絕；fixed-source inspection仍可使用。preview不建立持久狀態，apply與restore不得把 offline Library當作刪除訊號。

替代方案是先 commit deployment再寫檔，或 delete-then-rename；兩者都會暴露部分成功狀態，因此拒絕。

### Conflict-safe latest recovery restore

每個 profile／Project／Agent deployment只暴露最新有效 recovery point。`preview_config_profile_restore`重讀 current fixed target，以 typed diff呈現 current到backup中 allowlisted values的變化，token綁定 current fingerprint與 recovery revision；raw backup不跨 backend boundary。

`apply_config_profile_restore`只接受 token，在同一 write lock內重驗 current fingerprint與 recovery metadata。成功前先把 current bytes／absence保存為新的 recovery point，然後 atomic restore前一版本；backup kind為 absent時移除本次由 AgentDeck建立的 regular file，但 symlink／special file一律拒絕。成功後更新 deployment fingerprint與 status。

替代方案是提供歷史 backup瀏覽或任意 backup選擇；本階段只需可靠的一步 undo，較小的 authority也降低 secret exposure，因此拒絕。

### Config Profiles management UI state machine

頁面保留既有 inventory filters與 diagnostics，另以 profile list／editor／assignment區塊呈現 management。editor只由 backend提供的 allowlist metadata產生 typed controls；save前不建立 assignment。apply與restore均採 preview dialog，顯示 profile、Project、Agent、source與 typed diff，confirm送出 token，cancel不發 apply command。

每次 mutation成功後重新載入 profile list、assignments與inventory；`stale_preview`保留 editor選擇並要求重新 preview。loading期間阻止重複 confirm，latest request wins避免舊 response覆蓋新狀態。UI不渲染 user／local scope target、raw source、secret control、arbitrary path或跨專案 batch action。

替代方案是在 inventory table直接加入逐格 auto-save；它缺少一致 preview與可理解的 transaction boundary，因此拒絕。

## Implementation Contract

### Observable behavior

使用者可建立一個含 Codex／Claude Code allowlisted scalar的 named profile，把它分別指派給多個已登錄 Project與 Agent。每個 assignment都必須先顯示 fixed project target與 typed diff；只有確認同一 preview token才寫入。外部修改、invalid／symlink／oversized source、offline write gate或任一步失敗都顯示 stable error且不宣稱成功。最新 recovery point可先 preview再 restore。

### Interface / data shape

- Profile commands：`list_config_profiles`、`create_config_profile`、`update_config_profile`、`delete_config_profile`。
- Assignment commands：`set_config_profile_assignment`、`remove_config_profile_assignment`、`list_config_profile_assignments`。
- Apply commands：`preview_config_profile_apply(request)`與`apply_config_profile({ token })`。
- Restore commands：`preview_config_profile_restore(request)`與`apply_config_profile_restore({ token })`。
- Profile DTO：ID、name、revision、typed entries、created／updated timestamps；entry只有 Agent、canonical key與 string／boolean／integer scalar。
- Assignment DTO：profile ID、Project ID、Agent、fixed source ID、deployment status、optional last applied fingerprint／time、has recovery point。
- Preview DTO：token、operation、profile／revision、Project／Agent、source ID、base fingerprint或 absent、typed diff、expiry；不含 path、raw document或 backup bytes。
- Stable errors：`profile_not_found`、`project_not_found`、`invalid_profile_entry`、`profile_in_use`、`library_offline`、`source_invalid`、`unsupported_symlink`、`too_large`、`stale_preview`、`preview_expired`、`write_failed`、`atomic_replace_unsupported`、`rollback_failed`、`recovery_not_found`。

### Failure modes

- CRUD／assignment validation失敗：SQLite transaction rollback，Artifact、detail、entries與deployment rows不變。
- preview source missing：回 create preview；invalid、oversized、symlink或special file：回 stable error且不發 token。
- apply identity或revision不符：回 `stale_preview`，零 backup、零 source write、零 deployment update。
- apply fault：原 target bytes／absence與canonical deployment state回復；若回復失敗，回 `rollback_failed`且保留 owner-private recovery。
- restore current fingerprint或recovery revision不符：回 `stale_preview`，不改 current source或recovery pointer。
- UI cancel：不呼叫 apply／restore command，不改 source、SQLite或recovery state。

### Acceptance criteria

- migration tests驗證舊 schema原子升級、typed CHECK／FK／unique constraints與rollback後舊資料不變。
- Rust tests覆蓋 allowlist validation、CRUD、revision、assignment integrity、fixed targets、missing／invalid／oversized／symlink、preview token、stale conflict、unknown preservation、TOML comments、JSON nested siblings、owner-only backup、fault injection、rollback與restore。
- serialization tests證明 raw source、unknown key/value、secret、path、parser／OS error不出現在 profile／preview／error JSON或log。
- frontend `npm run build`、`npm run lint`、`npm run check:i18n`與`npm run check:config-profile-management`通過。
- 完整 `cargo test --locked`、`git diff --check`、`spectra analyze manage-codex-claude-config-profiles --json`與`spectra validate manage-codex-claude-config-profiles`通過。
- 人工 GUI驗證 create／edit／assign、apply success、cancel、stale preview、invalid source與restore；temporary registered projects前後 snapshot證明只改確認的fixed target與owner-private recovery state。

### Scope boundaries

In scope是ConfigProfile typed persistence、canonical Project／Agent assignment、Codex／Claude project target preview／apply、latest recovery restore與對應UI。Out of scope是user／local writes、secret storage、raw document UI、arbitrary path、batch／automatic apply、profile import/export、歷史backup browser與其他Artifact行為。

預期受影響檔案完整清單：

- `src-tauri/src/core/config_profile_management.rs`（新增）
- `src-tauri/src/commands/config_profile_management.rs`（新增）
- `scripts/check-config-profile-management.mjs`（新增）
- `src-tauri/src/core/artifact.rs`
- `src-tauri/src/core/config_profile_inventory.rs`
- `src-tauri/src/core/migrations.rs`
- `src-tauri/src/core/skill_store.rs`
- `src-tauri/src/core/mod.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/lib/tauri.ts`
- `src/views/ConfigProfiles.tsx`
- `src/i18n/en.json`
- `src/i18n/zh-TW.json`
- `package.json`
- `plan.md`
- `openspec/specs/config-profile-management/spec.md`（archive時新增）
- `openspec/specs/config-profile-inspection/spec.md`（archive時修改）
- `openspec/specs/product-board-interface/spec.md`（archive時修改）

## Risks / Trade-offs

- [Typed profile涵蓋範圍小] → 只接受既有allowlist；新增key必須先修改inspection spec與validation table。
- [跨filesystem與SQLite無單一transaction] → replace前持久化recovery，commit失敗時atomic rollback並以fault injection驗證每個邊界。
- [來源含未知secret] → raw bytes只存在bounded in-memory parser與owner-private recovery，不進一般DTO、SQLite payload、Library或log。
- [JSON重新serialize改變空白] → 保證unknown key/value語意保留，不宣稱JSON byte-for-byte formatting保留；typed preview只呈現allowlisted changes。
- [Project同時被外部工具修改] → fingerprint-bound single-use preview與global write lock縮小race；任何 mismatch要求重新preview。
- [Library offline阻止profile metadata write] → 明示`library_offline`且保留唯讀inspection，不fallback到其他路徑。

## Migration Plan

1. 新schema在單一SQLite migration transaction新增detail／entry／recovery metadata，既有Artifacts與deployments不改寫。
2. 啟動時先完成migration，再註冊management commands；migration失敗維持舊schema與唯讀inspection。
3. rollback code時保留新增tables；舊binary忽略它們，已寫入project設定可由最新recovery point在新版本restore。
4. 不自動建立profile、不把既有config匯入profile、不自動建立assignment或寫來源。

## Open Questions

無。Phase 6 plan已固定project-scope、allowlist-only、preview-first與latest-recovery邊界。
