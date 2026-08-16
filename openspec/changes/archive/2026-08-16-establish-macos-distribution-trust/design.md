## Context

Phase 7 以 `src-tauri/target/release/bundle/macos/AgentDeck.app` 與 DMG證明本機personal build可安裝，但現有`.github/workflows/release.yml`仍是上游release流程：release名稱為Skills Manager、檢查`skills-manager.app`、建立`latest.json`／`.sig`／`.app.tar.gz` updater assets，並以macOS／Linux／Windows matrix直接讓`tauri-action`建立draft release。這同時違反目前的AgentDeck identity、app updater缺席與Phase 8只建立macOS公開信任鏈的邊界。

現有workflow已有pinned actions、Apple credential名稱、Developer ID／hardened runtime／stapler／Gatekeeper檢查雛形，但build、upload與publish authority耦合，build job持有`contents: write`，而updater驗證會要求已由Phase 7移除的artifact。利害關係人是下載官方DMG的macOS使用者與執行tagged release的維護者。外部限制是Apple notarization與GitHub Environment必須由repository外部配置，實作不得把certificate、password或private key寫入Git或log。

2026-08-16 的後續範圍決策確認 AgentDeck 目前只供維護者本人使用，沒有外部下載者。這項決策不回寫或刪除已完成的 workflow、checker 與安全驗證成果，但取消目前範圍內的 live acceptance 與公開發佈；沒有新的 Spectra change 授權前，release path 保持未配置且未啟用。

## Goals / Non-Goals

**Goals:**

- 讓`v<version>`tag、tag commit、`package.json`、`src-tauri/tauri.conf.json`、Bundle ID、DMG名稱與embedded AgentDeck.app metadata形成可自動驗證的單一release identity。
- 讓macOS arm64與x86_64 build在公開前通過Developer ID Application、hardened runtime、notarization、stapling、Gatekeeper與SHA-256檢查。
- 將build、verification與publish authority分離；只有final publish job可寫GitHub Release，失敗只留下non-public draft或workflow artifact。
- 保留personal local build文件與`npm run check:personal-installation`，並新增official hosted release的獨立驗證說明。
- 以repository checker在沒有Apple secrets時也能用fixtures檢查workflow的identity、authority、secret、updater absence與publish ordering。
- 明確記錄 personal-only 操作範圍，使完成本 change 不需要配置 release credentials、推 tag 或建立 GitHub Release。

**Non-Goals:**

- 不新增application auto-update、`latest.json`、Tauri updater signing key、runtime release query、download或install flow。
- 不建立Windows／Linux公開artifact、platform signing或cross-platform release matrix；既有source與cross-platformtests不移除。
- 不建立Mac App Store、Sparkle、付費distribution、certificate申請或Apple Developer帳號自動化。
- 不把Apple或GitHub credentials儲存在AgentDeck Library、SQLite、Git backup、repository variable範例或可下載artifact。
- 不保證Apple notarization服務永遠可用；service timeout必須fail closed且不得公開未驗證artifact。
- 不在目前 personal-only 範圍配置 `macos-release` Environment、執行 tagged acceptance run，或建立 draft／public release。

## Decisions

### Staged macOS build, verification, and publication authority

`.github/workflows/release.yml`只接受`v*`tag push建立official release；`workflow_dispatch`最多執行non-publishing dry run，不能建立或修改release。top-level permission固定`contents: read`。兩個macOS build jobs在受保護`macos-release`Environment內建置arm64與x86_64，將signed／notarized DMG及metadata上傳為workflow artifacts；只有依賴全部build與verification的publish job取得`contents: write`。

publish job下載兩個DMG與checksum，先建立或更新同tag的draft、以authenticated API核對asset完整性及digest，再把同一draft轉成public。tag或release已存在時fail closed，不覆寫既有asset、不重指tag。build／verify失敗不呼叫publish job；publish驗證失敗保留draft供人工檢查但不公開。

替代方案是保留每個matrix leg內的`tauri-action`release upload。那會讓部分平台先完成時就擁有release寫入權限，且難以證明兩個architecture都完成後才公開，因此拒絕。

### Ephemeral Apple credentials and fail-closed secret handling

Developer ID certificate、certificate password、App Store Connect issuer、key ID與private key只由`macos-release`Environment secrets注入。每個macOS job建立runner-local temporary Keychain及owner-only private key file；cleanup step以`if: always()`刪除temporary Keychain與key file。workflow只輸出stable missing／invalid credential名稱，不輸出value、decoded content、Keychain dump或notary response中的敏感欄位。

`APPLE_TEAM_ID`作為受保護Environment variable提供expected TeamIdentifier；它不是credential，但仍由release environment管理以避免把不同team的有效Developer ID誤認為AgentDeck。任何credential或team identity缺失都在bundle build前失敗，禁止ad-hoc fallback。

替代方案是沿用repository-level secrets與Tauri的隱式fallback。那無法限制可使用secrets的job，也可能產生結構有效但不具公開信任的signature，因此拒絕。

### AgentDeck-only signed and notarized artifact verification

每個architecture固定驗證`AgentDeck.app`與同build DMG。checker先驗tag／version／Bundle ID，再對build app執行`codesign --verify --deep --strict`、檢查Developer ID authority、expected TeamIdentifier、timestamp與runtime flag，接著執行`xcrun stapler validate`及`spctl --assess --type execute`。DMG必須有stapled ticket；以read-only mount取得唯一`AgentDeck.app`後重跑相同identity與Gatekeeper checks，最後unmount。

DMG名稱固定含version與architecture；`shasum -a 256`產生一個同名`.sha256`，內容只含lowercase hex digest與asset basename。`latest.json`、`.sig`、`.app.tar.gz`或Skills Manager命名一律視為distribution finding。

替代方案是只驗build directory中的app或只依賴notary submission成功。前者無法證明使用者實際下載的DMG內容，後者無法證明ticket已staple且Gatekeeper接受，因此拒絕。

### Repository-owned distribution contract without live secrets

新增`npm run check:macos-distribution`，以Node.js standard library靜態解析committed workflows、Tauri／npm metadata與文件，輸出固定summary；其Node fixture tests用temporary repository tree覆蓋identity、tag、permission、Environment、secret scope、updater artifact、publish dependency、checksum與documentation failures。checker不連GitHub、不呼叫Apple、不讀operator Keychain或environment secret value。

live signing／notarization只能在受保護Environment內驗證；repository checker負責在PR階段證明workflow shape fail closed，tagged acceptance run再保存run URL、tag、commit、asset names、digests與Apple／Gatekeeper結果，不保存secret或machine path。

替代方案是只靠tagged run發現workflow錯誤。那會把拼字、舊bundle path與authority問題延遲到需要外部credentials的昂貴流程，因此拒絕。

### Personal build and official release remain separate trust channels

`README.md`保留Phase 7 personal local build段落，明載該artifact不繼承official signing或hosting trust；另以`docs/macos-distribution.md`說明official GitHub Release的tag、AgentDeck DMG、checksum、Developer ID、notarization與Gatekeeper驗證。`scripts/check-personal-installation.mjs`只評估local-build段落，不因repository存在official release文件而失敗；`scripts/check-no-upstream-app-updater.mjs`允許CI／documentation中的hosted release references，但runtime與build updater surfaces仍禁止release query、endpoint、public key或install flow。

替代方案是把personal與official步驟合成單一安裝說明。那會讓unsigned local artifact被誤認為已notarize，或讓official artifact看似沒有來源信任，因此拒絕。

### Personal-use scope leaves live distribution inactive

維護者已確認目前 App 只供本人使用，沒有要提供給其他人。因此 task 5.3 以「不啟用 live release」完成：不建立或配置 `macos-release` Environment credentials、不推 acceptance tag、不執行 Apple signing／notarization acceptance run，也不建立 draft 或 public GitHub Release。已完成的 workflow、checker 與 fixtures 保留為 dormant implementation；它們只能證明若未來啟用時會 fail closed，不能視為目前已有官方下載管道。

未來若使用範圍改成對外發佈，維護者必須先建立新的 Spectra change，重新檢查當時的 Apple／GitHub 要求、workflow 與文件，並把原 task 5.3 所列的雙架構 live acceptance 當成公開前條件。替代方案是本輪刪除所有已完成的 release 實作；這會把「取消目前 live release」擴大成未經要求的 rollback，因此本次 ingest 不採用。

## Implementation Contract

### Observable behavior

目前維護者只使用本機 personal build；repository 不因本 change 的完成而配置 release credentials、推新 tag、執行 signing／notarization acceptance run，或建立 GitHub Release。保留的 release workflow 在缺少受保護 Environment 時維持 fail closed，不能產生 release-ready artifact。

維護者push一個尚未發佈的`v1.31.0`tag時，workflow先證明tag commit位於受保護main history，且`package.json`與`src-tauri/tauri.conf.json`皆為`1.31.0`。arm64與x86_64 jobs使用相同commit，各自產生AgentDeck DMG與checksum；每個DMG內唯一app的Bundle ID為`io.github.yichin17.agentdeck`、版本為`1.31.0`、TeamIdentifier符合release Environment，並通過Developer ID、hardened runtime、stapler與Gatekeeper。只有兩組artifact與全部Phase 7 gates通過後，final job才公開標題為`AgentDeck v1.31.0`的GitHub Release。

失敗、取消或notary timeout不公開release。使用者可下載與architecture相符的DMG，重算SHA-256並依文件核對checksum，再由macOS Gatekeeper驗證official artifact；personal build仍使用原本本機檢查，且不宣稱official trust。

### Interface / data shape

- npm script：`check:macos-distribution`執行`node scripts/check-macos-distribution.mjs`。
- Checker成功摘要：`macOS distribution contract passed: product=AgentDeck targets=arm64,x86_64 updater=absent publish=staged`。
- Stable findings：`identity_mismatch`、`tag_version_mismatch`、`release_authority_too_broad`、`release_environment_missing`、`secret_boundary_violation`、`updater_asset_present`、`verification_gate_missing`、`checksum_missing`、`publish_order_invalid`、`documentation_incomplete`。
- Workflow artifacts每個architecture固定為一個DMG與一個`.sha256`；checksum line格式為`<64 lowercase hex><two spaces><DMG basename>`。
- Official release evidence只記錄GitHub Actions run URL、tag、commit SHA、DMG basenames、SHA-256、architectures及各gate pass／fail，不記錄certificate bytes、password、private key、issuer value、key ID value、notary raw log、home或runner temporary path。
- GitHub `macos-release`Environment需要secrets `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_API_ISSUER`、`APPLE_API_KEY`、`APPLE_API_KEY_BASE64`，以及variable `APPLE_TEAM_ID`；未配置時workflow在build前以缺少的名稱退出。
- 目前 personal-only 範圍的 completion evidence 是使用者在本次對話明確取消 live release，以及既有 `plan.md` 記錄尚未配置 Environment、尚未執行 tag、沒有公開 release；不建立 run URL、DMG digest 或 Apple credential evidence。

### Failure modes

- tag、version、commit或Bundle ID不一致：兩個build jobs都不開始signing，無draft或public release。
- credential缺少、decode失敗或certificate identity不符：job回stable credential／identity錯誤，不fallback到ad-hoc signing，不輸出secret value。
- notarization timeout、rejection、stapler或Gatekeeper失敗：該architecture job失敗，不上傳release-ready artifact，publish job不執行。
- 只有一個architecture完成、checksum不符、asset重複或含updater artifact：draft verification失敗且release保持non-public。
- 同tag或release已存在：workflow拒絕覆寫並要求新的version／tag；不自動刪除或重指歷史release。
- 已公開release事後需要撤回：維護者依文件將release轉回draft並保留tag與incident evidence；workflow不自動刪tag、revokecertificate或替換asset。
- personal-only 範圍被誤當成 release 授權：維護者不得為完成本 change 配置 credentials 或推 acceptance tag；必須先以新的 Spectra change 變更範圍。

### Acceptance criteria

- `node --test scripts/check-macos-distribution.test.mjs`涵蓋clean workflow及所有stable findings，`npm run check:macos-distribution`對repository exit 0。
- `npm run build`、`npm run lint`、`npm run check:i18n`、全部repository Node contracts、`cargo test --locked --manifest-path src-tauri/Cargo.toml`、兩個production audits及`npm run check:personal-installation`全部exit 0。
- Workflow syntax review證明top-level read-only、只有publish job可`contents: write`、build jobs使用`macos-release`Environment、沒有TAURI updater key或updater assets。
- 目前 personal-only 範圍以明確不配置受保護 Environment、不推 acceptance tag、不建立 draft／public release完成；若新的 Spectra change 日後授權對外發佈，才必須執行 arm64／x86_64 tagged acceptance run並核對下載後digest、identity、Developer ID、hardened runtime、stapler與Gatekeeper。
- 若未來授權公開，GitHub Release在全部gate通過前保持draft，公開後標題、tag、commit與assets皆符合AgentDeck identity；evidence通過secret／absolute-path scan。
- `spectra analyze establish-macos-distribution-trust --json`沒有Critical／Warning，`spectra validate establish-macos-distribution-trust`與`git diff --check`通過。

### Scope boundaries

In scope是保留`.github/workflows/prepare-release.yml`與`.github/workflows/release.yml`的AgentDeck macOS fail-closed信任鏈、repository distribution checker、personal／official trust區隔，以及將 task 5.3 關閉為不啟用 live release。Out of scope是配置Apple／GitHub release credentials、推 acceptance tag、建立公開release、刪除已完成的release實作、runtime application updater、其他平台public artifacts、Mac App Store、certificate申請、Apple帳務、產品runtime／SQLite／Library／Agent workflow改動。

## Risks / Trade-offs

- [Risk] Apple notarization長時間停在In Progress → 每個macOS job保留60分鐘timeout；timeout視為失敗且publish不執行，不重試到可能重複release。
- [Risk] GitHub draft upload在最後驗證前留下non-public assets → publish job以tag查找唯一draft、驗證完整集合後才轉public；失敗保留draft供人工檢查。
- [Risk] Intel與arm64使用不同runner時間點造成source drift → 兩者checkout同一`github.sha`並把commit寫入metadata；publish拒絕不同commit。
- [Risk] Environment配置錯誤但credential本身有效 → 以expected TeamIdentifier、AgentDeck Bundle ID與tag version共同驗證，不只接受任意Developer ID。
- [Risk] 將GitHub Release誤當runtime updater → repository checker拒絕`latest.json`、`.sig`、`.app.tar.gz`、Tauri updater key及runtime release query，official hosting只提供人工下載。
- [Trade-off] Phase 8只公開macOS，暫時少於上游跨平台release範圍 → 保留cross-platformsource與CI checks，待各平台有獨立signing／installation contract後再提案。

## Migration Plan

1. 先用fixture tests鎖定現有workflow的Skills Manager名稱、舊bundle path、updater artifacts與過寬publish authority為failing cases。
2. 將prepare／release workflow改成AgentDeck macOS staged pipeline，保持tag尚未觸發；以repository checker與現有regression驗證。
3. 目前 personal-only 範圍保持`macos-release`Environment未配置、不推新tag且不建立release；以本次範圍決策關閉live acceptance。
4. 未來只有在新的Spectra change授權對外發佈後，才建立受保護Environment、配置列出的secrets／variable，並以尚未使用的新version tag執行acceptance run。
5. 若未來acceptance失敗，保留或轉回draft、不要重用tag；修正需用新patch version與新tag重跑。若implementation尚未公開任何release，rollback只需還原workflow與文件，不影響runtime data。

## Open Questions

無。目前固定為personal-only且不啟用live release；對外發佈、application auto-update與其他平台distribution都必須由後續獨立change決定。
