## Why

Phase 7 已證明 AgentDeck 可由本機安全建置與安裝，但既有 GitHub release workflow 仍承襲上游 Skills Manager 名稱、跨平台 matrix 與 updater artifacts，且簽章／notarization gate 檢查的是舊 bundle path。若直接推 tag，可能產生名稱錯誤、要求已移除的 updater assets，或在 macOS trust 尚未完整驗證時公開 artifact，因此 Phase 8 必須先建立 AgentDeck 專屬的 macOS 公開發佈信任鏈。

2026-08-16 的範圍更新確認目前 App 只供維護者本人使用，沒有對外發佈對象。因此已完成的 release 安全管線保留為未啟用的備用實作，但不配置 `macos-release` credentials、不推 acceptance tag，也不建立 draft 或 public GitHub Release；未來若改為對外發佈，必須另開 Spectra change 重新確認當時的信任需求並完成 live acceptance。

## What Changes

- 將 tagged release 收斂為 macOS arm64／x86_64 的 AgentDeck `.app` 與 DMG，固定 tag、`package.json`、`src-tauri/tauri.conf.json`、Bundle ID、commit與asset版本一致。
- 以受保護的 GitHub Environment 提供 Developer ID Application與App Store Connect credentials；缺少或格式錯誤時在build前fail closed，且secret內容不得進入artifact、cache、log或文件。
- 對build output與DMG內的app執行Developer ID、hardened runtime、notarization ticket、Gatekeeper、identity與version驗證；只有全部gate及Phase 7 regression通過才公開GitHub Release。
- 移除release workflow對`latest.json`、`.sig`、`.app.tar.gz`與runtime updater signing key的要求，保留application updater缺席；改為發佈DMG與對應SHA-256 checksum。
- 將release名稱、release notes與artifact檢查從Skills Manager修正為AgentDeck，並把personal local build與official signed／notarized hosted release文件分開。
- 新增repository-owned distribution checker與fixtures，使舊bundle path、舊產品名、updater assets、未受保護secret scope、先公開後驗證、缺少checksum或Gatekeeper gate都會以stable finding失敗。
- 將目前操作範圍固定為 personal-only：task 5.3 不執行 live release acceptance，保留的 workflow 與 checker 不構成目前的公開發佈承諾或授權。

## Capabilities

### New Capabilities

- `macos-distribution-trust`: 定義AgentDeck macOS官方release的tag／identity／signing／notarization／Gatekeeper／checksum／publish gate與secret邊界，並要求 personal-only 範圍下保持未啟用。

### Modified Capabilities

- `app-update-policy`: 保持 personal installation 為目前 release policy；保留但未啟用的 GitHub Release 實作不得成為公開發佈或 application auto-update trust root。
- `product-identity`: 將AgentDeck identity延伸到tagged release名稱、notes、bundle paths與公開assets，禁止沿用Skills Manager release identity。

## Impact

- Affected phase: `plan.md` Phase 8 macOS公開發佈信任鏈。
- Affected specs: `macos-distribution-trust`、`app-update-policy`、`product-identity`。
- Affected code and documents:
  - New: `scripts/check-macos-distribution.mjs`、`scripts/check-macos-distribution.test.mjs`、`scripts/prepare-release.test.mjs`、`docs/macos-distribution.md`。
  - Modified: `.github/workflows/prepare-release.yml`、`.github/workflows/release.yml`、`scripts/prepare-release.mjs`、`scripts/check-no-upstream-app-updater.mjs`、`scripts/check-no-upstream-app-updater.test.mjs`、`scripts/check-personal-installation.mjs`、`scripts/check-personal-installation.test.mjs`、`package.json`、`README.md`、`plan.md`、`.gitignore`（讓`docs/macos-distribution.md`可被committed）。
  - Removed: none.
- External systems: GitHub Actions protected environment、GitHub Releases、Apple Developer ID certificate、Apple notarization service；目前 personal-only 範圍不配置或呼叫這些 live release 資源。
- Dependencies: 沿用Node.js standard library、GitHub Actions既有pinned actions、Tauri CLI與macOS內建`codesign`／`spctl`／`xcrun stapler`／`shasum`；不新增production dependency。
- Intentional divergence: 官方release workflow只發佈macOS AgentDeck assets，不再發佈上游Windows／Linux assets或application updater metadata；source與既有跨平台tests保留。
- Secret boundary: certificate、password、private key、issuer、key ID與GitHub token只由runner environment取得，不進repository或可下載artifact。
