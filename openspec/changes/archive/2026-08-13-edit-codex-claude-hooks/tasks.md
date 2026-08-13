## 1. TDD 契約與失敗注入

- [x] 1.1 先為「Hook mutation resolves only fixed writable sources」與 design「固定 source capability 與可寫檔案邊界」在 `src-tauri/src/core/hook_management.rs` 建立 failing tests，固定 known／unknown source id、known／unknown Project、existing／missing／offline root、regular file／symlink／special file結果；逐支執行 `hook_management::tests::fixed_source_*`，確認實作前至少一支因module或API尚不存在而 FAILED。
- [x] 1.2 先為「Agent-specific operations validate before transformation」及「Round trips preserve configuration outside edited fields」與 design「Patch operation DTO 與 Agent-specific validation」建立 Codex／Claude Code create／update／delete、stale locator、unknown field、JSON sibling與TOML comments／order fixtures；執行 `hook_management::tests::operation_*` 與 `hook_management::tests::round_trip_*`，固定只有被選取欄位改變且 `sentinel-secret` 不進preview／error。
- [x] 1.3 先為「Preview binds an exact source revision to validated operations」及「Apply is conflict-safe, recoverable, and atomic」與 design「Preview revision 與精確衝突檢查」和「Recovery backup 與 atomic replacement transaction」建立missing revision、SHA-256 conflict、262,144／262,145-byte、4,000／4,001-line及backup／stage／replace／SQLite commit fault injection tests；執行 `hook_management::tests::preview_*` 與 `hook_management::tests::apply_*`，確認所有失敗點保持原bytes或absence。
- [x] 1.4 先為「Hook identity and backup metadata exclude Hook payload」、「Restore requires preview and preserves a reverse recovery point」及「Interrupted Hook writes recover before new mutations」與 design「Hook Artifact identity 與 schema v9 metadata」和「Conflict-safe restore 與單一 recovery point」建立populated v8 migration、fresh schema、sentinel SQLite dump、bytes／absence restore、reverse recovery與unfinished journal fixtures；執行 `migrations::tests::test_v9_*`、`skill_store::tests::hook_*` 與 `hook_management::tests::restore_*`／`recovery_*`，固定實作前契約失敗。

## 2. Source operations、round-trip 與 preview

- [x] 2.1 依「固定 source capability 與可寫檔案邊界」在 `src-tauri/src/core/hook_management.rs` 重用descriptor與Project lookup，實作typed source capability、`HookEditOperationDto`、locator與Agent-specific validation，使frontend path、symlink、offline root及unknown values fail closed；以1.1及1.2 validation tests證明request只可到達固定source。
- [x] 2.2 依「Patch operation DTO 與 Agent-specific validation」實作JSON完整document patch與TOML `DocumentMut` node-level create／update／delete，使non-Hook siblings、unknown Hook values、comments與order在未修改處保留；以1.2 round-trip fixtures逐值／逐comment比較，並執行 `cargo test --manifest-path src-tauri/Cargo.toml --locked hook_management::tests::round_trip -- --nocapture`。
- [x] 2.3 依「Preview revision 與精確衝突檢查」實作full-source SHA-256、missing revision、`HookWritePreviewDto`、validation issues與256 KiB／4,000-line gate，使preview不寫filesystem／SQLite且只回傳Hook subtree；以1.3 preview tests比較before／after、`canApply`、`wouldCreateFile`及所有boundary值。

## 3. Persistence、atomic apply 與 recovery

- [x] 3.1 依「Hook Artifact identity 與 schema v9 metadata」在 `src-tauri/src/core/migrations.rs`、`src-tauri/src/core/skill_store.rs`與`src-tauri/src/core/artifact.rs`加入schema v9、`hook_details`／`hook_backups` constraints及store APIs，使first successful apply建立且後續重用kind `hook` identity，preview／failed apply不建立row；以1.4 migration／store tests驗證v8無損、fresh parity、rollback、idempotence與zero seed rows。
- [x] 3.2 依「Recovery backup 與 atomic replacement transaction」實作 `central_repo::base_dir()/hook-backups` owner-private latest payload、same-directory staged target、fsync、Unix atomic replace、mode preservation與unsupported-platform fail-closed，使apply在backup後才替換且不使用delete-then-rename；以1.3 fault injection tests逐點斷言target、backup與metadata一致。
- [x] 3.3 依「Recovery backup 與 atomic replacement transaction」實作payload-free operation journal、SQLite commit補償與startup reconciliation gate，使crash後在新mutation前回復或完成一致狀態，reconciliation失敗只封鎖mutation並保留inspection；以1.4 `recovery_*` tests模擬replace前後及commit前後journal。
- [x] 3.4 依「Conflict-safe restore 與單一 recovery point」實作latest metadata query、restore preview、base revision gate、bytes／absence atomic restore與reverse recovery point，使corrupt／stale backup不改目前source；以1.4 `restore_*` tests驗證apply→restore→reverse restore與其他source／parent directories不變。

## 4. Tauri contract 與 Hooks UI

- [x] 4.1 在 `src-tauri/src/commands/hooks.rs`、`src-tauri/src/core/mod.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`及`src/lib/tauri.ts`註冊並型別化 `preview_hook_change`、`apply_hook_change`、`get_hook_recovery`、`preview_hook_restore`、`apply_hook_restore`，讓IPC只接受project id、source id與typed requests，並以command serialization tests固定所有error keys且證明payload不進log／database。
- [x] 4.2 依「Hooks UI 的 draft、preview 與 apply 狀態機」，將「Hooks page exposes filters, diagnostics, details, and compatibility without mutation controls」改為「Hooks page exposes gated Hook editing without execution controls」；在 `src/components/HookEditor.tsx`與`src/views/Hooks.tsx`實作Agent-specific form、unknown read-only rows、`editing → previewing → preview_ready → applying → applied`、draft invalidation、latest-request-wins、restore preview與source capability gates，以 `npm run build`及manual rapid Project switch證明stale response／preview不覆蓋新context，且無Execute／Test controls。
- [x] 4.3 更新 `scripts/check-hooks-ui.mjs`、`src/i18n/en.json`、`src/i18n/zh-TW.json`與`package.json`，讓static contract固定五個commands、Apply-before-preview prohibition、route-local sensitive state、source refusal reasons、Restore preview及locale parity；執行 `npm run check:hooks-ui`與`npm run check:i18n`要求exit 0。

## 5. 完整驗證與安全證據

- [x] 5.1 執行 `cargo test --manifest-path src-tauri/Cargo.toml --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:hooks-ui`與`git diff --check`，要求所有commands exit 0；再用temporary HOME／linked Project完成JSON／TOML preview、external edit conflict、apply、restore與crash-journal recovery，逐值比較non-Hook內容、SQLite dump、Library tree、Git status、logs與localStorage contract，確認只source／當次IPC／private recovery payload可含 `sentinel-secret`，且沒有Hook被執行。
