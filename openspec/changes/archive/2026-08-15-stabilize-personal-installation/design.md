## Context

Phase 0–6 已完成 894 個 Rust tests、frontend build／lint／i18n、各功能 contract與人工 GUI流程，但這些證據大多在 development runtime內。Phase 7 必須回答另一個問題：由鎖定依賴產出的 AgentDeck macOS bundle是否真的能以固定 identity啟動、沿用既有 `.skills-manager`／SQLite／backup contract、在 external Library offline時安全呈現，並讓使用者知道如何安裝、備份、還原與解除安裝。

現有 `app-update-policy` 已禁止 runtime updater與 upstream binary trust；`product-identity` 已固定 `AgentDeck`、Bundle ID `io.github.yichin17.agentdeck`及 legacy持久 contract；各 Skill／Plugin／Hook／Config Profile規格已定義 fixed authority、preview-first、rollback與 offline邊界。本 change不重新設計它們，而是建立 packaged build可重複驗收與個人安裝文件，只有具體 regression證據才能開啟最小 runtime修正。

利害關係人是從本機原始碼建置並安裝 AgentDeck的使用者與維護者。限制是不得新增 production dependency、不得提交 generated bundle或機器絕對路徑、不得接觸真實 token／credential、不得把個人安裝擴張成公開 distribution信任鏈，且 macOS-first驗證不得破壞既有跨平台程式碼。

## Goals / Non-Goals

**Goals:**

- 以 repository-owned checker驗證 packaged `.app`／installer存在、Bundle ID／版本正確、updater surface缺席且個人安裝文件完整。
- 以既有 Library／SQLite fixture及 temporary registered Codex／Claude Projects完成首次啟動、migration、Online／Offline與主要 workflow smoke，保存不含敏感或機器特定資料的證據。
- 讓 `README.md`提供可執行的本機 build、安裝、首次啟動、既有資料沿用、Library offline、backup／restore與解除安裝步驟。
- 任何既有 contract regression都先有 failing test與確切受影響路徑，經 artifacts更新後才做最小修正。

**Non-Goals:**

- 不建立公開 release、對外 distribution、release hosting、Developer ID signing、notarization、App Store或 application auto-update。
- 不新增或改變 Skill、Plugin、Hook、Config Profile、Library、Git backup或 CLI產品能力。
- 不重新命名或搬移既有 Library、SQLite、Git backup、Keychain、localStorage或 CLI contract。
- 不提交 `.app`、`.dmg`、build cache、使用者資料、token、credential或machine-specific absolute path。
- 不宣稱 Windows／Linux installer已完成；只保護現有跨平台 source與configuration不被macOS smoke改壞。

## Decisions

### Fixed packaged-artifact verification and deterministic output

新增 `npm run check:personal-installation`，固定檢查 `src-tauri/target/release/bundle/macos/AgentDeck.app`與 Tauri當次在 `src-tauri/target/release/bundle/`產生的 macOS installer。checker使用 Node.js standard library及 macOS內建 `plutil`／`codesign` inspection能力，不下載外部資料、不接受 update endpoint、不修改 bundle。

checker驗證 `Info.plist`的 `CFBundleIdentifier = io.github.yichin17.agentdeck`、`CFBundleName = AgentDeck`、bundle version與 `src-tauri/tauri.conf.json`一致、main executable存在且可執行、installer只封裝同一 identity，並重用 `check-no-upstream-app-updater`確認 dependency／permission／endpoint／public key／runtime install flow皆不存在。成功輸出一行穩定摘要；missing artifact、identity mismatch、version mismatch、missing executable、updater regression或documentation contract失敗以各自stable code回 non-zero。

替代方案是只信任 Tauri exit 0或掃描任意使用者提供的 app path。前者沒有驗證產物；後者會讓驗收範圍不固定，因此拒絕。

### Layered regression and packaged smoke evidence

驗收分成三層。第一層是鎖定依賴與repository suites：frontend build／lint／i18n、Node contracts、完整 Rust tests、JavaScript及Rust production audits。第二層是既有資料 fixture：從已知舊 schema／Library metadata啟動最新 core，驗證 migration後 ids、rows、files、backup metadata與legacy namespaces不變，external Library offline不建立fallback。第三層才是 packaged app smoke：啟動 `.app`，以temporary registered Projects操作 Skill sync／conflict、Plugin preview、Hook與Config Profile preview／cancel／apply／restore，並檢查Online／Offline顯示。

`docs/personal-installation-verification.md`保存驗證日期、macOS版本／架構、commit、相對artifact paths、每個command的exit與test count、smoke checklist結果及已知非阻擋warning。文件不得包含home path、temporary directory、Library實際路徑、source原文、token、credential或Keychain內容。

替代方案是只做人工 smoke或只做unit tests。單獨任一層都無法同時證明reproducibility與packaged behavior，因此拒絕。

### Regression fixes require artifact ingestion before runtime edits

提案不預測不存在的runtime bug。apply先執行既定checks；若失敗，保留實際error／log／重現步驟，使用 `spectra-ingest stabilize-personal-installation`把確切observable failure、受影響project-relative files、failing test名稱與修正後acceptance target寫回design／spec／tasks，再依TDD red-green-refactor修正。沒有具體失敗證據時，不修改runtime source。

替代方案是在tasks列出「修所有問題」或預先納入大量runtime paths。這會讓scope不可追溯，因此拒絕。

#### 已定位的Phase 7 regression

第一輪執行task 2.1／2.2的checks時出現三個具體失敗，皆已附實際輸出與重現指令，因此依本decision先ingest再修正：

1. `node --test scripts/check-legacy-compatibility.test.mjs` 回57 pass／2 fail：`no parallel AgentDeck protocol tree, refs, storage keys or Keychain service exists`與 `legitimate AgentDeck identifiers are not treated as a parallel namespace`。違規字串是 `src-tauri/src/core/config_profile_inventory.rs` 中Config Profile隔離測試寫出的替身檔名 `agentdeck.db`，同時觸發 `parallel-database`與 `parallel-localstorage-key`規則。該替身代表的是既有SQLite database，因此正確作法是改用legacy檔名 `skills-manager.db`，而不是放寬legacy namespace規則。
2. `npm run check:hooks-ui` 回exit 1、`no Hook command may take a filesystem path from the frontend`。`scripts/check-hooks-ui.mjs` 把 `src/lib/tauri.ts` 從 `previewHookChange` 切到檔尾當作Hook參數面，於是Phase 6之後才加在後方的Config Profile回應型別欄位 `ConfigSource.displayPath` 被誤判成Hook參數。
3. `npm run check:plugins-ui` 回exit 1、`no Plugin command may take Path from the frontend`與 `no Plugin command may take env from the frontend`。`scripts/check-plugins-ui.mjs` 同樣從 `getPluginInventory` 切到檔尾，因此掃到同一個 `displayPath` 欄位與Config Profile註解中的 "environment" 字樣。

第2、3項是checker掃描範圍隨檔案順序漂移造成的false positive，不是Hook／Plugin授權邊界真的鬆掉。修正方向是把掃描範圍從「切到檔尾」改成「只取具名wrapper的宣告區塊」，讓規則與宣告在檔案中的位置無關，並以fixture證明真正的path／env參數仍會被攔下。

### Personal installation documentation preserves policy and data boundaries

`README.md`新增本機個人安裝段落，說明鎖定依賴build、實際artifact位置、將 `.app`移入 `/Applications`或個人 Applications目錄、首次啟動、既有Skills Manager資料沿用、external Library reconnect、Git backup／restore與解除安裝。解除安裝將app bundle與使用者資料分開說明；預設移除app不刪Library／SQLite／backup credential，任何資料清理都逐項列出且由使用者明確執行。

文件必須明載本機personal build沒有app auto-update、公開release hosting、Developer ID signing或notarization保證，並保留upstream attribution與實際legacy external names。不得提供繞過Gatekeeper、停用系統安全檢查或刪除廣泛home目錄的指令。

替代方案是沿用上游已簽章release說明。那會把上游binary trust誤套到本機fork，因此拒絕。

## Implementation Contract

### Observable behavior

維護者從乾淨checkout使用lockfiles執行完整驗收及 `npm run tauri:build`後，repository checker會辨識固定AgentDeck `.app`與macOS installer、驗證identity／version／executable／updater absence／documentation，並以exit 0與穩定摘要回報。使用者依 `README.md`可安裝bundle、首次啟動沿用既有資料、辨識Library Offline、使用Git backup／restore，並在解除安裝時分辨app與保留資料。

Packaged smoke必須證明：existing internal Library正常開啟；existing external Library離線時維持同一configured identity且零fallback mutation；temporary Codex／Claude Projects的Skill、Plugin、Hook與Config Profile主要流程符合既有preview／conflict／rollback contract。任何failed check不會被標記完成，也不會以刪測試、略過audit或擴大fallback取得pass。

### Interface / data shape

- npm script：`check:personal-installation`執行 `node scripts/check-personal-installation.mjs`。
- Node tests：`node --test scripts/check-personal-installation.test.mjs`使用temporary fixture驗證clean與各stable failure code。
- Checker固定輸入：`src-tauri/tauri.conf.json`、`package.json`、`README.md`、`scripts/check-no-upstream-app-updater.mjs`、`src-tauri/target/release/bundle/macos/AgentDeck.app`及同一bundle root下的macOS installer；不接受network URL、credential、arbitrary command或runtime update設定。
- 成功摘要：`Personal installation check passed: app=AgentDeck.app identifier=io.github.yichin17.agentdeck version=<version> updater=absent docs=complete`。
- Stable failures：`bundle_missing`、`installer_missing`、`identity_mismatch`、`version_mismatch`、`executable_missing`、`updater_surface_present`、`documentation_incomplete`、`unsupported_host`。
- Evidence document：`docs/personal-installation-verification.md`固定包含Environment、Artifacts、Automated checks、Packaged smoke、Data compatibility、Warnings六節；paths皆相對project root，check結果含command、exit、pass／fail count或build result。

### Failure modes

- build或suite失敗：保留command、exit與concise error；不產生成功evidence、不勾選task、不修改runtime直到artifacts完成ingest。
- `.app`／installer缺少或metadata不符：checker回stable code與project-relative artifact location；不搜尋home或其他build目錄當fallback。
- unsupported host：非macOS執行packaged check回`unsupported_host`；repository unit／contract tests仍可跑，Phase 7 packaged acceptance維持未完成。
- external Library offline：packaged app顯示Library Offline且不建立default Library、不刪deployment／backup state；reconnect由使用者明確觸發既有Retry流程。
- smoke外部修改或stale token：沿用既有typed conflict／`stale_preview`，不重試自動寫入。
- evidence包含secret或machine-specific absolute path：documentation check失敗且文件不得提交。

### Acceptance criteria

- `npm ci`、`npm run build`、`npm run lint`、`npm run check:i18n`、所有repository Node contracts、完整locked Rust tests皆exit 0，並記錄實際counts。
- `npm audit --omit=dev`與 `cargo audit`對production graph回0個active vulnerability；需要breaking remediation時另開change，不在此scope升級。
- `npm run tauri:build` exit 0；`npm run check:personal-installation`與其Node tests exit 0，metadata符合interface contract。
- migration／legacy compatibility／offline regression tests與temporary Project smoke全部通過；packaged `.app`啟動後主要頁面可讀，app退出後沒有遺留非預期mutation。
- `README.md`與verification evidence涵蓋個人安裝、首次啟動、資料沿用、Library Offline、backup／restore、解除安裝及no-auto-update政策。
- `git diff --check`、`spectra analyze stabilize-personal-installation --json`與 `spectra validate stabilize-personal-installation`無Critical／Warning且change valid。

### Scope boundaries

In scope是Phase 7 regression execution、由具體失敗驅動的最小修正、macOS local bundle build／inspection／smoke、personal installation文件與evidence。Out of scope是public release／distribution、signing、notarization、hosting、updater trust及未由失敗證據定位的新功能或runtime重構。

預期受影響檔案完整清單：

- `scripts/check-personal-installation.mjs`（新增）
- `scripts/check-personal-installation.test.mjs`（新增）
- `scripts/frontend-argument-surface.mjs`（新增，具名wrapper參數面的共用純函式）
- `scripts/check-ui-command-arguments.test.mjs`（新增，Hook／Plugin wrapper參數面的fixture證明）
- `docs/personal-installation-verification.md`（新增）
- `README.md`
- `package.json`
- `scripts/check-no-upstream-app-updater.test.mjs`
- `scripts/check-hooks-ui.mjs`（已定位regression 2）
- `scripts/check-plugins-ui.mjs`（已定位regression 3）
- `src-tauri/src/core/config_profile_inventory.rs`（已定位regression 1，只改測試fixture檔名）
- `plan.md`
- `.gitignore`（apply時發現：`docs` 整個目錄原本被忽略，evidence文件無法提交；改為 `docs/*` 加上該檔的negation，其餘docs內容維持忽略）
- `openspec/specs/personal-installation-readiness/spec.md`（archive時新增）
- `openspec/specs/app-update-policy/spec.md`（archive時修改）

上列三個已定位regression之外的runtime修正仍為none；若後續驗收再找到問題，必須先ingest並把確切路徑加入此清單。

## Risks / Trade-offs

- [Risk] macOS bundle format或Tauri installer target因工具版本改變 → checker從committed Tauri config與實際bundle root解析固定identity，只允許明確列出的macOS artifact種類，格式改變時以test更新contract。
- [Risk] packaged smoke誤觸真實Library或Agent設定 → 使用隔離的temporary home／Library／registered Projects與fake CLI adapters；任何無法隔離的人工步驟只做read-only inspection。
- [Risk] tracked evidence很快過期 → 文件記錄commit與日期，後續code change不自動宣稱仍有效；新的personal build須重跑checker與smoke。
- [Risk] unsigned local build被誤認為公開release → README與checker摘要只使用personal installation語意，不產生release upload、signing或notarization claim。
- [Trade-off] macOS-first packaged acceptance不證明Windows／Linux installer → 保留cross-platform suites並明確不宣稱其他平台完成。

## Migration Plan

1. 先建立failing Node contract，鎖定checker command、stable errors、documentation sections與no-updater邊界。
2. 實作checker與文件骨架，執行locked repository suites、audits及existing-data fixture regression。
3. 執行Tauri production build與packaged metadata check，再以隔離資料完成app smoke並填入evidence。
4. 若任何runtime regression出現，先ingest確切failure與paths，再以failing test驅動最小修正並重跑全套。
5. 更新Phase 7狀態；所有acceptance通過後archive。Rollback只移除checker／文件變更；runtime修正若存在則以其個別regression test保護，不能以downgrade SQLite或刪除使用者資料回退。

## Open Questions

無。公開distribution相關決策保留給未來獨立change。
