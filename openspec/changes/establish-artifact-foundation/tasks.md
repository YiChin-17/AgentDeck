## 1. TDD 契約與 migration fixtures

- [x] 1.1 先為「Artifact identity is typed and separate from subtype details」與 design「Artifact identity 與 typed detail 分離」在 `src-tauri/src/core/artifact.rs`／`src-tauri/src/core/skill_store.rs` tests 建立 failing cases，固定四個 `ArtifactKind` persisted values、unknown kind rejection、Skill parent/detail atomicity、kind mismatch與cascade isolation；逐支執行 named focused tests，確認實作前至少一支因型別或schema尚不存在而 FAILED。
- [x] 1.2 先為「Schema v7 upgrades are atomic, lossless, and retryable」與 design「Schema v8 transaction migration 與明確 downgrade 邊界」在 `src-tauri/src/core/migrations.rs` 建立真實 v7 fixtures，包含 Skills、`skill_targets`、Tags、Scenarios、Agent toggles、Projects、settings、audit log與pending conflicts，並新增 populated upgrade、fresh、idempotent、newer-schema及forced invariant failure tests；以row dump與`sqlite_master` snapshot確認實作前 upgrade／rollback cases FAILED且fixture本身可重現。
- [x] 1.3 先為「Deployment records represent scope and execution state explicitly」與 design「Canonical deployment storage 與 Skill compatibility API」建立 failing store tests，固定global／project scope、`symlink`／`copy`／`cli-managed`、uniqueness、invalid scope／mode、secret-free columns及legacy target逐欄mapping；以named focused tests確認新deployment API與table尚未實作時 FAILED。

## 2. Artifact schema 與原子 migration

- [x] 2.1 依「Artifact identity 與 typed detail 分離」在`src-tauri/src/core/artifact.rs`與`src-tauri/src/core/mod.rs`實作`ArtifactKind`、`ArtifactRecord`、`ArtifactScope`及`ArtifactDeploymentRecord`的strict conversion，使unknown kind／scope／mode回傳明確錯誤而不fallback；以1.1與1.3的enum、scope及mode focused tests全部通過驗證。
- [x] 2.2 依「Schema v8 transaction migration 與明確 downgrade 邊界」在`src-tauri/src/core/migrations.rs`建立`artifacts`、`artifact_deployments`、`skills.artifact_id`、indexes與invariant triggers，先backfill Skills及legacy targets，再驗證counts與foreign keys，只有成功後移除`skill_targets`並提交user_version 8；以populated／fresh migration tests斷言ids與欄位逐值相同、legacy table只在成功路徑消失。
- [x] 2.3 完成同一migration決策的safe failure：任何constraint、count或`PRAGMA foreign_key_check`錯誤都rollback到完整v7，成功v8重跑為no-op且schema v7 binary guard拒絕v8；以forced failure前後`sqlite_master`、row dump與user_version比較，以及既有`test_newer_schema_rejected`驗證沒有partial table或資料變動。

## 3. Canonical store 與 Skill 相容層

- [x] 3.1 依「Canonical deployment storage 與 Skill compatibility API」在`src-tauri/src/core/skill_store.rs`加入generic Artifact／deployment CRUD，強制`(artifact_id, scope_type, scope_id, agent)`唯一、valid scope／mode／enabled及foreign keys，並讓status／last_error寫入前沿用既有sanitize邊界；以1.3的兩種scope、三種mode、uniqueness、invalid input與secret column inspection tests驗證。
- [x] 3.2 依「Artifact identity 與 typed detail 分離」將`insert_skill`、`upsert_skill`與`delete_skill`改為單一transaction維持kind=`skill` parent、detail與owned deployment一致，保留`SkillRecord` serialization shape且不更名`SkillStore`；以atomic failure、kind mismatch、delete cascade、Tags／Scenario isolation與existing Skill CRUD tests驗證。
- [x] 3.3 依「Canonical deployment storage 與 Skill compatibility API」把`insert_target`、`get_targets_for_skill`、`get_all_targets`、`delete_target`及tool-key remap queries切到`artifact_deployments`，只將global enabled rows映射為原`SkillTargetRecord`；以field-by-field mapping、global disabled／project exclusion、scenario_service與agent_workspace regression tests驗證既有callers無observable diff。
- [x] 3.4 加入production source assertion，要求除migration與其fixtures外不再查詢`skill_targets`，並確認`SkillRecord`／`SkillTargetRecord` frontend及CLI JSON沒有新增required fields；以source assertion、`npm run build`與temporary root的`npm run cli -- --json --skills-root <tmp> skills list`／`presets list`輸出shape驗證漏改raw SQL會明確FAILED。

## 4. Backup、offline 與完整相容驗證

- [x] 4.1 依「Legacy Git backup format remains unchanged in Phase 3」與design「Legacy backup protocol 保持原格式」在`src-tauri/src/core/sync_metadata.rs`及`src-tauri/src/core/merge/integration_tests.rs`加入upgrade前後canonical bytes、protocol 2 restore／merge fixtures，證明reindex會重建kind=`skill` identity但不新增Artifact metadata目錄、不改schema／protocol／refs／trailers；以byte comparison與現有merge integration suite驗證。
- [x] 4.2 依「Existing Skill behavior and offline safety remain compatible」與design「Offline、conflict 與 secret 邊界沿用既有 contract」驗證offline external Library啟動可只升級internal SQLite，所有Library／deployment filesystem mutation仍回傳`library_offline`且前後path hash、target rows及Library identity不變；以focused app_state／library_availability tests及direct deployment command tests驗證。
- [x] 4.3 執行`cargo test --manifest-path src-tauri/Cargo.toml --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:board`、`npm run check:board-layout`、`npm run check:skill-pack-ui`、`npm run check:product-identity`、`npm run check:no-app-updater`、`npm run cli:build`與`git diff --check`，要求Rust 0 failed、所有commands exit 0，並以`git diff`確認沒有Hook／Plugin／Config Profile功能、frontend IPC、adapter路徑、backup protocol、dependency或legacy identifier變更。
