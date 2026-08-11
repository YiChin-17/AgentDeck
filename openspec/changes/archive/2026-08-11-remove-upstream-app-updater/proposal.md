## Why

AgentDeck 是不對外發布安裝檔的個人開發 App，但目前啟動時會查詢上游 Skills Manager release，並保留可下載及安裝由上游私鑰簽署之更新的完整路徑。上游版本不代表 AgentDeck 相容版本，這條信任路徑可能用上游 binary 覆蓋本機 AgentDeck，因此應在目前 Phase 1 收尾後以獨立安全修正移除。

## What Changes

- 移除啟動時與 Settings 手動觸發的上游 App release 查詢、通知及 release 連結。
- 移除 Settings 內的 App 下載、安裝與重新啟動更新流程；AgentDeck 不提供 App 自我更新入口。
- 移除 Tauri updater plugin、`updater:default` capability、updater endpoint、上游 pubkey 與 updater artifacts 產出設定。
- 移除只供 App updater 使用的前後端型別、IPC command、i18n 文案及 JavaScript／Rust dependencies。
- 新增可重複執行的來源檢查，確保執行期更新路徑不再指向 `xingkongliang/skills-manager`，同時不移除 README、BASELINE 與 MIT License 所需的上游來源標示。
- 將 `plan.md` Phase 7 調整為「穩定化與個人安裝」，把公開發佈、notarization 與 auto-update 記為未來若改變發佈策略才另行評估。

## Non-Goals

- 不建立 AgentDeck GitHub Release、自有 updater endpoint、簽章金鑰或 CI 發佈管線。
- 不同步或合併 upstream `v1.31.0`，也不變更人工 fetch／merge／cherry-pick 的上游維護方式。
- 不進行 AgentDeck 改名、圖標替換、Bundle ID 變更或資料路徑遷移；這些由 `establish-agentdeck-product-identity` 處理。
- 不移除上游 attribution、MIT License、fork baseline 或開發文件中的上游 repository URL。
- 不改變 Skill、Plugin 或其他 Artifact 自身的來源更新與版本檢查功能。

## Capabilities

### New Capabilities

- `app-update-policy`: 定義不對外發布的 AgentDeck 不查詢、下載或安裝 App binary 更新，並將上游同步限制在開發流程。

### Modified Capabilities

(none)

## Impact

- Affected plan: `plan.md` Phase 7；本安全修正安排在 Phase 1 完成與歸檔後執行，不納入 `protect-offline-external-library`。
- Intentional upstream divergence: AgentDeck 移除上游既有 App updater，仍保留 Git upstream 與必要 attribution。
- Affected specs: `app-update-policy`
- Affected code:
  - Modified: `src-tauri/tauri.conf.json`
  - Modified: `src-tauri/capabilities/default.json`
  - Modified: `src-tauri/src/lib.rs`
  - Modified: `src-tauri/src/commands/settings.rs`
  - Modified: `src-tauri/Cargo.toml`
  - Modified: `src-tauri/Cargo.lock`
  - Modified: `src/context/AppContext.tsx`
  - Modified: `src/views/Settings.tsx`
  - Modified: `src/lib/tauri.ts`
  - Modified: `src/i18n/en.json`
  - Modified: `src/i18n/zh-TW.json`
  - Modified: `package.json`
  - Modified: `package-lock.json`
  - Modified: `plan.md`
  - New: `scripts/check-no-upstream-app-updater.mjs`
- Affected checks: React／TypeScript production build、ESLint、Rust tests、i18n integrity check，以及新的 updater source check。
- Data and secrets: 不修改 Library、SQLite、Git backup、Keychain 或使用者設定；不產生或保存簽章私鑰。
