<!--
Each task description MUST state:
- the behavior or contract being delivered (what is observably true when the
  task is complete), and
- the verification target that proves completion (test, CLI invocation,
  analyzer check, manual assertion, or content review).

File paths are supporting context for locating the work, never the task
itself. "Edit file X" is not a valid task — it is missing both behavior and
verification.
-->

## 1. Backend contract tests

- [x] 1.1 先在 `src-tauri/src/core/config_profile_inventory.rs` 建立 failing tests，證明「固定來源與 Project identity」及 **Inventory reads only fixed supported sources**：無 project 時只檢查 user sources、registered project 只組合固定路徑、unknown ID 回 `project_not_found` 且零讀取；以指定 test names 執行 `cargo test --locked config_profile_inventory::tests::fixed_sources` 驗證先 red 後 green。
- [x] 1.2 先建立 failing tests，證明「Bounded parser 與來源隔離」及 **Source reads are bounded and isolated**：missing、1 MiB boundary、too large、unreadable、symlink、invalid TOML／JSON 各自隔離且不執行 CLI；執行 `cargo test --locked config_profile_inventory::tests::source_isolation` 驗證先 red 後 green。
- [x] 1.3 先建立 failing serialization tests，證明「明確 scalar allowlist 與 DTO boundary」、**Only exact non-sensitive scalar settings cross the backend boundary** 及 **Inventory exposes source identity without source content**：合法 scalar 正規化、錯誤型別 fail closed、secret／unknown／raw error 不在 JSON、成功來源才有正確 SHA-256；執行 `cargo test --locked config_profile_inventory::tests::dto_boundary` 驗證先 red 後 green。
- [x] 1.4 先建立 failing precedence tests，證明「Supported-source precedence 與唯讀 diff」及 **Precedence and diff are limited to supported sources**：Claude user→project→local、Codex project candidate、same／added／changed／removed 均只使用 allowlisted typed values；執行 `cargo test --locked config_profile_inventory::tests::supported_precedence` 驗證先 red 後 green。
- [x] 1.5 先建立零副作用 tests，證明「唯讀頁面與不變的 persistence」及 **Inspection produces no persistent side effects**：load／refresh 前後 temporary home、project、Library、SQLite 與 Application Support snapshot 不變，Library offline 不觸發同步；執行 `cargo test --locked config_profile_inventory::tests::no_persistent_side_effects` 驗證先 red 後 green。

## 2. Backend inventory implementation

- [x] 2.1 實作固定 source descriptor、registered project lookup 與 symlink-safe metadata 流程，使 request 無法攜帶 path 且 source status 符合「Fixed sources and Project identity」的 **Interface / data shape**；以 task 1.1 tests 與 request serde rejection tests 全部通過驗證。
- [x] 2.2 實作 1 MiB bounded read、`toml_edit`／`serde_json` parser、per-source stable status 與 sanitized diagnostic，使所有 **Failure modes** 只隔離單一來源且不回傳 raw error；以 task 1.2 tests 與 `cargo test --locked config_profile_inventory::tests::source_isolation` 通過驗證。
- [x] 2.3 實作 exact Agent-specific allowlist、typed `ConfigSettingDto`、`ConfigSourceDto`、`ConfigProfileInventoryDto` 與 SHA-256 fingerprint，使 backend response 符合 **Observable behavior** 且未知／敏感內容在 serialization 前已被丟棄；以 task 1.3 tests 及 response JSON forbidden-string assertions 通過驗證。
- [x] 2.4 實作 supported-source resolution 與 normalized diff，使 Claude 依 user→project→local 排序、Codex project 為 `project_candidate` 且不宣稱完整 runtime effective config；以 task 1.4 table-driven tests 通過驗證。
- [x] 2.5 註冊 `get_config_profile_inventory` command 及 module exports，讓 frontend 只能以 optional project／Agent／scope filters 取得 inventory，並維持「Scope boundaries」不新增 migration、write command 或 production dependency；以 command integration tests、`cargo test --locked config_profile_inventory` 與 `cargo check --locked` 通過驗證。

## 3. Frontend inspection workflow

- [x] 3.1 先新增 `scripts/check-config-profiles-ui.mjs` 的 failing static contract，涵蓋 **Config Profiles page is inspection-only** 與 **Existing specialized workflows remain available**：route／sidebar／filters／diagnostics／runtime limitation 必須存在，create／edit／assign／apply／backup／restore wrapper 必須不存在；以 `npm run check:config-profiles-ui` 驗證先失敗。
- [x] 3.2 在 `src/lib/tauri.ts` 新增與 serde response 一致的 request／DTO types 及 read-only wrapper，使 frontend 無法提交 path、cwd、environment、raw config 或 mutation；以 `npm run build` 的 TypeScript compile 與 static contract 中的 forbidden-wrapper assertions 通過驗證。
- [x] 3.3 新增 `src/views/ConfigProfiles.tsx` 的 Agent／scope／registered-project filters、refresh、source status、typed diagnostics、normalized setting table、diff 與 runtime limitation 說明，使 Implementation Contract 的 **Observable behavior** 可由使用者完整看見；以 `npm run check:config-profiles-ui` 與 manual fake-IPC valid／missing／invalid render assertions 通過驗證。
- [x] 3.4 更新 `src/App.tsx`、`src/components/Sidebar.tsx` 與兩份 i18n，使 Config Profiles 導航只開啟 inspection page、所有文案有 en／zh-TW 對應且不出現 mutation control；以 `npm run check:i18n`、`npm run lint`、`npm run build` 與 task 3.1 static contract 通過驗證。

## 4. Acceptance and scope verification

- [x] 4.1 依 **Acceptance criteria** 執行 `cargo test --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:config-profiles-ui` 與 `git diff --check`，記錄 test counts 與每個 command exit 0；任何失敗必須保留實際輸出，不得標記完成。
- [x] 4.2 以 temporary home／registered project 人工驗證 GUI valid、missing、invalid、refresh 四條流程，並比較前後 snapshot 證明真實 `~/.codex`、`~/.claude`、Library、Application Support、SQLite 與 system secret storage 零寫入；這項 manual assertion 同時驗證 **Inspection produces no persistent side effects**。
- [x] 4.3 審查 **Scope boundaries**：確認 diff 僅含 proposal 的完整 affected-file list、沒有 ConfigProfile persistence／mutation／assignment／backup／restore／managed policy／CLI resolution／environment resolution，再執行 `spectra analyze inspect-codex-claude-config-profiles --json` 與 `spectra validate inspect-codex-claude-config-profiles` 驗證無 Critical／Warning 且 change valid。
