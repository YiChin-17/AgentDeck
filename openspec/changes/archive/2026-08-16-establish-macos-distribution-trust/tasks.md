## 1. Repository distribution contract

- [x] 1.1 先在`scripts/check-macos-distribution.test.mjs`建立failing temporary-tree fixtures，逐一覆蓋 **Repository checks enforce the distribution contract without live credentials** 的clean summary與`identity_mismatch`、`tag_version_mismatch`、`release_authority_too_broad`、`release_environment_missing`、`secret_boundary_violation`、`updater_asset_present`、`verification_gate_missing`、`checksum_missing`、`publish_order_invalid`、`documentation_incomplete`；以`node --test scripts/check-macos-distribution.test.mjs`驗證先red，且fixtures不得讀network、Keychain或environment secret value。
- [x] 1.2 依 **Repository-owned distribution contract without live secrets** 使用Node.js standard library實作`scripts/check-macos-distribution.mjs`與`package.json`的`check:macos-distribution`，使repository輸出固定成功摘要、所有1.1 findings轉green，並以source assertion證明checker沒有network、Keychain、subprocess signing或secret-value讀取。

## 2. Release identity and authority

- [x] 2.1 依 **Tagged macOS releases have one traceable AgentDeck identity** 與 **User-facing desktop identity is AgentDeck**，讓`scripts/prepare-release.mjs`及`.github/workflows/prepare-release.yml`只建立版本一致的AgentDeck tag commit，修正locale file清單並拒絕tag重用／非main history；以新增的prepare-release fixtures、dry run diff及`npm run check:product-identity`驗證tag、`package.json`、`src-tauri/tauri.conf.json`與release名稱一致。
- [x] 2.2 依 **Staged macOS build, verification, and publication authority** 重構`.github/workflows/release.yml`為tag-triggered arm64／x86_64 macOS build、verification、final publish三段，top-level與build jobs保持`contents: read`且只有依賴全部gates的publish job擁有`contents: write`；以1.1 authority／ordering fixtures及workflow dependency review證明`workflow_dispatch`不會publish、單一architecture不會建立public release。
- [x] 2.3 依 **Release credentials are ephemeral and fail closed** 與 **Ephemeral Apple credentials and fail-closed secret handling**，讓兩個build jobs只從受保護`macos-release`Environment取得列出的Apple secrets與`APPLE_TEAM_ID`，建立temporary Keychain／owner-only key file並以`if: always()`cleanup，缺少／invalid／team mismatch時在artifact前退出；以fixtures驗證無repository-level secret fallback、無value logging、無TAURI updater key、無ad-hoc artifact。

## 3. Artifact trust and publication

- [x] 3.1 依 **Every distributed application is signed, notarized, stapled, and Gatekeeper-approved** 與 **AgentDeck-only signed and notarized artifact verification**，讓每個architecture job驗證build `AgentDeck.app`及read-only mounted DMG內唯一app的Bundle ID／version／Developer ID／TeamIdentifier／timestamp／hardened runtime／stapler／Gatekeeper，並驗證DMG ticket；以workflow fixtures、project-relative artifact assertions及一次local unsigned negative run證明任一gate缺少或失敗都不產生release-ready artifact。
- [x] 3.2 依 **Publication is staged, complete, and checksum-verifiable**，為每個DMG產生固定basename的`.sha256`，讓final job只接受兩個architectures、兩個checksums、同commit metadata與零updater assets，先核對authenticated draft再publish且拒絕覆寫既有tag／release／asset；以digest fixtures、asset-set table tests與publish-order contract轉green驗證。

## 4. Trust-channel documentation and policy

- [x] 4.1 依 **Personal installation is the documented release policy** 調整`scripts/check-no-upstream-app-updater.mjs`／tests與`scripts/check-personal-installation.mjs`／tests，使CI／official distribution文件可提GitHub Release、Developer ID與notarization，但runtime／build updater surface與personal local build仍拒絕release query、endpoint、public key、`latest.json`、`.sig`、`.app.tar.gz`及official trust繼承；以兩組Node fixtures與`npm run check:no-app-updater`、`npm run check:personal-installation`驗證。
- [x] 4.2 依 **Users can distinguish personal and official trust channels** 與 **Personal build and official release remain separate trust channels** 更新`README.md`並建立`docs/macos-distribution.md`，讓使用者可依architecture下載AgentDeck DMG、重算SHA-256、確認Gatekeeper與理解withdrawal，同時personal build仍清楚標示無official trust；以documentation topic／Gatekeeper-bypass／secret／absolute-path fixtures及人工content review驗證。

## 5. End-to-end acceptance and handoff

- [x] 5.1 執行所有distribution／identity／updater／personal-installation Node contracts與workflow syntax review，對照Implementation Contract的 **Observable behavior**、**Interface / data shape**、**Failure modes**、**Acceptance criteria**、**Scope boundaries**，證明stable summaries／findings、file formats、fail-closed ordering與in／out scope一致；以所有commands exit 0及artifact diff不超出proposal Impact驗證。
- [x] 5.2 重跑`npm ci`、`npm run build`、`npm run lint`、`npm run check:i18n`、全部repository Node contracts、`cargo test --locked --manifest-path src-tauri/Cargo.toml`、`npm audit --omit=dev`、`cargo audit`、`npm run check:personal-installation`與`git diff --check`，要求全部exit 0且dependency graph、Library／SQLite／Agent runtime source與cross-platform tests沒有未授權變更。
- [x] 5.3 依 **Personal-use scope leaves live distribution inactive** 與 **Personal-use scope keeps live distribution inactive**，接受使用者在2026-08-16確認AgentDeck只供本人使用、沒有外部發佈對象的範圍決策：不配置`macos-release`Environment credentials、不推acceptance tag、不執行arm64／x86_64 live signing／notarization run，也不建立draft或public GitHub Release；以本次conversation context及既有`plan.md`記錄Environment未配置、tagged acceptance未執行、沒有公開release驗證，未來對外發佈前必須另開Spectra change並完成live acceptance。
- [x] 5.4 更新`plan.md` Phase 8實際結果與rollback／withdrawal狀態，執行`spectra analyze establish-macos-distribution-trust --json`與`spectra validate establish-macos-distribution-trust`，要求無Critical／Warning、所有requirements／decisions有task coverage、change valid，且不得以刪測試、放寬secret boundary或重新加入updater取得pass。
