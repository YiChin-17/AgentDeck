<!--
Each task description MUST state:
- the behavior or contract being delivered (what is observably true when the
  task is complete), and
- the verification target that proves completion (test, CLI invocation,
  analyzer check, or manual assertion).

File paths are supporting context for locating the work, never the task
itself.
-->

## 1. Packaged artifact checker contract

- [x] 1.1 先在 `scripts/check-personal-installation.test.mjs` 建立 failing fixture tests，覆蓋 **Packaged artifacts have fixed identity and no updater authority**、**Fixed packaged-artifact verification and deterministic output** 及 **Interface / data shape**：clean AgentDeck `.app`／installer、`bundle_missing`、`installer_missing`、`identity_mismatch`、`version_mismatch`、`executable_missing`、`updater_surface_present`、`documentation_incomplete`、`unsupported_host`，並驗證成功摘要的app／identifier／version／updater／docs欄位；以 `node --test scripts/check-personal-installation.test.mjs` 驗證先red。
- [x] 1.2 使用Node.js standard library與macOS內建metadata inspection實作 `scripts/check-personal-installation.mjs`及 `package.json`的 `check:personal-installation`，使checker只讀固定bundle root、絕不network／mutation／home fallback，且task 1.1所有stable outputs轉green；以 `node --test scripts/check-personal-installation.test.mjs`、`npm run check:no-app-updater`與source assertions通過驗證。

## 2. Existing-data and workflow regression gates

- [x] 2.1 依 **Existing data survives first launch of the packaged application** 與 **Layered regression and packaged smoke evidence** 重跑schema 0→latest／populated migration、legacy compatibility、internal Library及external Library offline tests，證明ids／rows／relationships／backup metadata／legacy namespaces不變且零fallback mutation；以 `cargo test --locked --manifest-path src-tauri/Cargo.toml migration`、`node --test scripts/check-legacy-compatibility.test.mjs`及offline test filters的實際counts／exit 0驗證。
- [x] 2.2 依 **Packaged smoke preserves established workflow safety** 重跑temporary registered Codex／Claude Project的Skill sync／conflict、Plugin preview、Hook preview／cancel／apply／restore與Config Profile preview／cancel／apply／restore suites，證明只有confirmed fixed targets變更、stale／cancel零mutation、recovery exact round-trip；以各既有named Rust／Node contracts的exit 0及affected-file snapshot assertions驗證，且不得呼叫真實Plugin CLI或讀取operator資料。
- [x] 2.3 執行 **Regression fixes require artifact ingestion before runtime edits** gate：彙整task 2.1／2.2實際error／log／重現步驟並確認runtime regression為none；若有失敗，保持本task未完成，先用 `$spectra-ingest stabilize-personal-installation`把observable failure、failing test與確切project-relative paths加入artifacts，禁止以未定位runtime修改、fallback、刪測試或略過檢查取得pass；以artifacts diff與失敗輸出review驗證。
- [x] 2.4 依 **Existing data survives first launch of the packaged application** 的legacy namespace邊界，讓 `src-tauri/src/core/config_profile_inventory.rs` 中「inspection不觸碰Library／SQLite／Application Support」的隔離測試改用legacy database檔名 `skills-manager.db` 當替身（目前用的 `agentdeck.db` 會在repository內引入parallel database與localStorage namespace），且該測試仍斷言三棵樹byte-for-byte不變；以 `node --test scripts/check-legacy-compatibility.test.mjs` 由57 pass／2 fail轉為全數pass、`cargo test --locked --manifest-path src-tauri/Cargo.toml config_profile` 維持132 passed／0 failed驗證，不得改寬 `check-legacy-compatibility.test.mjs` 的parallel namespace規則。
- [x] 2.5 依 **Packaged smoke preserves established workflow safety** 先在新的 `scripts/check-ui-command-arguments.test.mjs` 建立failing fixture，證明Hook wrapper真的接受filesystem path時會被攔下、而宣告在Hook wrapper之後的Config Profile回應型別欄位不會被誤判；再把 `scripts/check-hooks-ui.mjs` 的參數面從「`previewHookChange` 切到檔尾」改成只取 `getHookInspection`、`previewHookChange`、`applyHookChange`、`getHookRecovery`、`previewHookRestore`、`applyHookRestore` 六個具名wrapper的宣告區塊；以fixture test先red後green及 `npm run check:hooks-ui` exit 0驗證。
- [x] 2.6 同樣以 `scripts/check-ui-command-arguments.test.mjs` 的fixture先red，證明Plugin wrapper真的接受 `Path`／`executable`／`args`／`cwd`／`env` 時會被攔下、而後方Config Profile型別與註解不會被誤判；再把 `scripts/check-plugins-ui.mjs` 的參數面從「`getPluginInventory` 切到檔尾」改成只取 `getPluginInventory`、`previewPluginMutation`、`applyPluginMutation` 三個具名wrapper的宣告區塊；以fixture test轉green及 `npm run check:plugins-ui` exit 0驗證。

## 3. Personal installation documentation

- [x] 3.1 先擴充 `scripts/check-no-upstream-app-updater.test.mjs`與task 1.1 documentation fixtures，使 **Personal installation is the documented release policy**、**Installation documentation and evidence are complete and non-sensitive** 及 **Personal installation documentation preserves policy and data boundaries** 在缺少local build、first launch、legacy data reuse、Library Offline、backup／restore、uninstall、no-auto-update任一主題時red，並拒絕Gatekeeper bypass、public distribution trust及machine-specific／secret evidence；以兩個Node test files驗證先red。
- [x] 3.2 更新 `README.md`並建立 `docs/personal-installation-verification.md`六節證據格式，讓使用者可完成本機build／安裝／首次啟動／existing data reuse／Library reconnect／backup／restore／app與data分離解除安裝，且文件明載無auto-update／public hosting／signing／notarization保證；以task 3.1 tests、project-relative path scan與secret-pattern scan轉green驗證。

## 4. Locked suites and production audits

- [x] 4.1 依 **Locked regression gates establish install readiness** 執行 `npm ci`、`npm run build`、`npm run lint`、`npm run check:i18n`、所有repository Node contracts與 `cargo test --locked --manifest-path src-tauri/Cargo.toml`，把command、exit、frontend build result與實際pass／fail counts填入evidence；要求全部exit 0且不得沿用plan中的assumed count。
- [x] 4.2 對committed production graphs執行 `npm audit --omit=dev`與 `cargo audit`，將0 active vulnerability及工具版本記入evidence；若需breaking upgrade或behavior change，保持task未完成並另提Spectra change，不在本change更動dependency graph；以兩個audit exit 0與manifest／lockfile diff review驗證。

## 5. Local bundle and packaged smoke

- [x] 5.1 依 **Observable behavior** 執行 `npm run tauri:build`，確認實際產生 `src-tauri/target/release/bundle/macos/AgentDeck.app`及同build的macOS installer，記錄project-relative artifact paths、version與build warnings，不追蹤generated output；以build exit 0、artifact existence及 `git status --short`無bundle檔驗證。
- [x] 5.2 執行 `npm run check:personal-installation`，依 **Failure modes** 證明Bundle ID `io.github.yichin17.agentdeck`、AgentDeck name、version、executable、installer、docs與updater absence全部成立；以stable成功摘要、exit 0及task 1.1完整fixture tests通過驗證。
- [x] 5.3 以隔離temporary home／Library／registered Projects／fake Plugin adapters啟動packaged `.app`，依 **Acceptance criteria** 人工驗證main pages、existing internal Library、external Library Offline／Retry、Skill、Plugin、Hook及Config Profile smoke，並依 **Scope boundaries** 確認operator真實設定／Library／Keychain零存取、app退出後只保留預期temporary mutation；以before／after snapshots與completed evidence checklist驗證。

## 6. Final acceptance and durable handoff

- [x] 6.1 重跑 `npm run build`、`npm run lint`、`npm run check:i18n`、所有Node contracts、完整locked Rust tests、兩個production audits、`npm run check:personal-installation`與 `git diff --check`，review evidence無secret／absolute path／generated bundle且affected files不超出Implementation Contract；最後執行 `spectra analyze stabilize-personal-installation --json`與 `spectra validate stabilize-personal-installation`，要求無Critical／Warning、change valid並更新 `plan.md` Phase 7實際counts／完成狀態，任何失敗不得標記完成。
