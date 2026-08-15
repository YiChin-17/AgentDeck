<!--
Each task description MUST state:
- the behavior or contract being delivered (what is observably true when the
  task is complete), and
- the verification target that proves completion (test, CLI invocation,
  analyzer check, or manual assertion).

File paths are supporting context for locating the work, never the task
itself.
-->

## 1. Typed persistence and assignments

- [x] 1.1 先在 `src-tauri/src/core/migrations.rs` 與 `src-tauri/src/core/skill_store.rs` 建立 failing migration／constraint tests，覆蓋 **Profiles persist only exact typed non-sensitive settings**：舊 schema 原子升級、detail／entry FK、unique key、互斥 scalar CHECK 與 rollback 後舊資料不變；以指定 test names 執行 `cargo test --locked config_profile_management_migration` 驗證先 red 後 green。
- [x] 1.2 依 **Typed ConfigProfile persistence 與 canonical assignment integrity** 實作 schema、store transaction 與 profile service，使 **Profile CRUD is revisioned and transactionally consistent**：create revision 1、update exact increment、stale revision／invalid entry 零變更、in-use profile 不可刪除；以 `cargo test --locked config_profile_management::tests::profile_crud` 與 direct-SQL constraint tests 全部通過驗證。
- [x] 1.3 先測後實作 canonical deployment assignment commands，使 **Assignments reuse canonical Project deployments**：profile／Project／Agent tuple 唯一、unknown Project 拒絕、assign／unassign 不寫 source、protected recovery 不被移除；以 `cargo test --locked config_profile_management::tests::assignment_integrity` 與 row-count assertions 通過驗證。

## 2. Fixed targets and preview authority

- [x] 2.1 先建立 failing authority tests，證明 **Mutation resolves only fixed Project sources** 與 **Fixed project target authority 與 explicit scope exclusion**：只解析 registered Project 的 Codex `.codex/config.toml`／Claude `.claude/settings.json`，missing 可 preview，unknown Project、invalid、1 MiB over-limit、symlink、special file 拒絕且零旁路讀取；以 `cargo test --locked config_profile_management::tests::fixed_project_targets` 驗證先 red 後 green。
- [x] 2.2 實作 preview store、expiry／single-use consume 與 commands，使 **Apply requires an exact single-use typed preview**、**Revision-bound typed preview token** 及 **Interface / data shape** 成立：token 綁定 profile revision、Project、Agent、source fingerprint／absent、output hash與 typed diff，apply request 只能含 token；以 `cargo test --locked config_profile_management::tests::preview_authority` 的 success／stale source／stale profile／expired／replay cases通過驗證。
- [x] 2.3 建立 serde與response forbidden-field tests並完成 command wiring，證明 path、scope、raw document、unknown key、secret、parser／OS error無法進 request／DTO／log，且 stable errors符合 contract；以 `cargo test --locked config_profile_management::tests::dto_boundary`、command integration tests與 source-string assertions通過驗證。

## 3. Preservation transforms and atomic apply

- [x] 3.1 先建立 table-driven failing round-trip tests，覆蓋 **Agent-specific transformation preserves unselected content** 與 **Agent-specific preservation transform**：Codex selected key更新但 comments／ordering／unknown tables不變，Claude `permissions.defaultMode`更新但 nested siblings／env不變，profile omission不刪 existing key；以 `cargo test --locked config_profile_management::tests::preservation_transform` 驗證先 red 後 green。
- [x] 3.2 實作 TOML／JSON selected-entry transform與post-transform verification，使 preview／output只有 allowlisted typed diff，missing target產生最小合法 document，invalid output在 staged write前 fail closed；以 task 3.1 tests及 byte／semantic preservation assertions全部通過驗證。
- [x] 3.3 先建立每個 fault point 的 failing tests，覆蓋 **Apply is atomic, recoverable, and state-consistent** 與 **Atomic apply、deployment state 與 failure rollback**：recovery promotion、staged sync、atomic replace、post-write verify、SQLite commit、rollback failure及 unsupported platform；以 `cargo test --locked config_profile_management::tests::atomic_apply_faults` 驗證先 red 後 green。
- [x] 3.4 實作 global write lock、owner-private recovery、same-directory staged file／sync／atomic replace、post-write verification、deployment commit與atomic rollback，使 **Failure modes** 的每個失敗都保留原 bytes／absence或明確回 `rollback_failed`，Library offline在 persistent mutation前回 `library_offline`而 inspection仍可用；以 task 3.3 tests、Unix permission assertion與 `cargo test --locked config_profile_management::tests::offline_write_gate` 通過驗證。

## 4. Conflict-safe restore

- [x] 4.1 先建立 failing latest-recovery tests，覆蓋 **Restore is previewed and conflict-safe** 與 **Conflict-safe latest recovery restore**：existing bytes restore、absent marker移除 AgentDeck-created regular file、current fingerprint conflict、missing recovery、symlink／special file拒絕及 raw backup不進 DTO；以 `cargo test --locked config_profile_management::tests::restore_contract` 驗證先 red 後 green。
- [x] 4.2 實作 restore preview／single-use token／apply commands，讓成功 restore先保存 current snapshot再 atomic還原 previous snapshot並更新 deployment，stale或fault時 source／recovery pointer／deployment不變；以 task 4.1 tests與 apply→restore→restore round-trip通過驗證。

## 5. Explicit management UI

- [x] 5.1 先新增 `scripts/check-config-profile-management.mjs` 的 failing static contract，涵蓋 **Config Profiles page is inspection-only** 的 removal／migration、**Config Profiles page separates inspection and management**、**Existing specialized workflows remain available** 與 **Config Profiles management is explicit and cancelable**：既有 inventory不退化、typed editor／assignment／preview confirm／restore存在，user／local／secret／path／auto／batch controls不存在，cancel不呼叫 apply；以 `npm run check:config-profile-management` 驗證先失敗。
- [x] 5.2 依 **Config Profiles management UI state machine** 實作 `src/lib/tauri.ts` types／wrappers、`src/views/ConfigProfiles.tsx` profile CRUD／assignment／apply／restore dialogs與兩份i18n，讓 stale preview保留選擇並要求重review、成功後完整refresh、double confirm被封鎖；以 `npm run build`、`npm run lint`、`npm run check:i18n`、task 5.1 contract及 manual fake-IPC success／cancel／stale render assertions通過驗證。

## 6. Acceptance and durable handoff

- [x] 6.1 依 **Acceptance criteria** 執行 `cargo test --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:config-profiles-ui`、`npm run check:config-profile-management` 與 `git diff --check`並記錄test counts／exit 0；再依 **Observable behavior** 人工驗證 temporary registered Projects 的create／edit／assign、apply success、cancel、stale、invalid與restore，依 **Scope boundaries** 確認只改confirmed fixed target與owner-private recovery、affected-file list無scope leakage，最後執行 `spectra analyze manage-codex-claude-config-profiles --json` 與 `spectra validate manage-codex-claude-config-profiles`，要求無Critical／Warning且change valid；任何失敗必須保留實際輸出且不得標記完成。
