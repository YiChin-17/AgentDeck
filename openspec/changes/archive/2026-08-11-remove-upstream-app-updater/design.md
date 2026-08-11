## Context

AgentDeck 仍繼承上游 Skills Manager 的兩段 App 更新流程：`check_app_update` 在啟動後與 Settings 手動檢查時查詢上游 GitHub Releases API；使用者選擇安裝後，Tauri updater plugin 依 `src-tauri/tauri.conf.json` 的上游 endpoint 下載 artifact，並用同檔案中的上游 pubkey 驗證。`AppContext` 負責啟動通知，`Settings` 負責檢查與安裝，前後端 dependency、Tauri capability 及 bundle updater artifacts 共同維持這條路徑。

AgentDeck 的產品方向已與上游不同，且目前決策是不對外發布 App。Git `upstream` 仍用於開發者人工檢查與承接程式碼，但上游 release 不能作為 AgentDeck binary 的更新來源。這是獨立於 `protect-offline-external-library` 的安全修正，待該 change 完成與歸檔後才實作。

## Goals / Non-Goals

**Goals:**

- 讓執行中的 AgentDeck 不再主動查詢任何 App binary release。
- 移除下載、簽章驗證、安裝及重啟套用 App 更新的 UI 與 runtime 能力。
- 移除 updater 專用 dependencies、Tauri permission 與 build artifacts，縮小不需要的信任面。
- 用 repository-owned 檢查區分合法的上游 attribution 與禁止的 runtime updater reference。
- 將 `plan.md` 的 Phase 7 改為個人安裝策略，不承諾公開發佈與 auto-update。

**Non-Goals:**

- 不建立自有 release、endpoint、keypair、signing secret 或 CI pipeline。
- 不改 Git remotes、不同步 upstream tag、不限制開發者人工承接上游程式碼。
- 不改 App 名稱、圖標、Bundle ID、CLI 名稱、Library 或 backup protocol。
- 不移除 Skill／Plugin 等使用者內容的來源版本檢查。
- 不改 SQLite、設定格式、Keychain 或外部 Library availability 行為。

## Decisions

### 完整移除 App updater，而不是只改 endpoint

移除 backend release query、frontend 通知與安裝入口、Tauri updater plugin 及 build 設定。因為 AgentDeck 不對外發布，保留空 endpoint、隱藏按鈕或 feature flag 都沒有 caller，會留下誤啟用與維護成本。

替代方案是改指向 `YiChin-17/AgentDeck`，但沒有 release artifacts 與私鑰簽章流程時無法形成可用且可驗證的更新管線，因此不採用。另一替代方案是只移除安裝按鈕並保留通知；上游版本與 AgentDeck 版本不具可比較語意，通知仍會誤導，因此也不採用。

### 上游同步保留在開發流程

保留 `origin`／`upstream`、`BASELINE.md`、README attribution 與 MIT License。開發者透過 Git 比較、選擇性 merge 或 cherry-pick，再執行 AgentDeck 驗證；執行中的 App 不參與此流程。

替代方案是刪除所有上游 URL，但會破壞來源追溯與授權資訊，不採用。

### 用限定範圍的靜態檢查防止更新路徑回歸

新增無第三方 dependency 的 Node script，檢查 updater runtime／build surfaces：Tauri config、capability、Rust plugin registration與 release query、frontend updater import／IPC／startup notification、dependency manifests。檢查禁止上游 release API、updater endpoint、updater pubkey、`tauri-plugin-updater` 與 updater permission 回到這些 surfaces。

檢查不掃描 README、BASELINE、License、change artifacts、Git remote 說明或其他 attribution 文件，避免把合法來源記錄誤判為 runtime 信任。它也不禁止 Skill／Plugin source URL。

替代方案只靠人工 review，無法阻止未來同步上游時 updater 被帶回，因此不採用。

### 個人安裝策略取代公開發佈承諾

`plan.md` Phase 7 保留 stabilization、本機 build、資料備份與解除安裝驗證；公開 release、notarization 與 auto-update 改為未來發佈策略變更時另開 change 評估。這不阻止 Tauri 產生一般 `.app` 或平台安裝包，只停止 updater artifacts。

## Implementation Contract

- **Runtime behavior:** App 啟動完成後不得因 App 版本檢查向 GitHub Releases 發出請求，不顯示 App binary 新版通知；Settings 不提供檢查、下載、安裝或重啟套用 App 更新的控制項。
- **IPC surface:** 移除 `check_app_update` command、`AppUpdateInfo` DTO、frontend `checkAppUpdate` wrapper，以及只供該流程使用的 context state。Tauri invoke handler 不再註冊該 command。
- **Updater surface:** Tauri builder 不註冊 updater plugin；capability 不包含 updater permission；Tauri config 不包含 updater endpoint／pubkey，bundle 不產生 updater artifacts；JavaScript 與 Rust manifests／lockfiles不包含 updater plugin dependency。
- **Failure behavior:** 因功能不存在，不建立「無法連線更新服務」錯誤或 fallback。GitHub 無法連線不影響 App 啟動及 Settings。一般 Git backup、Skill source update 與 Plugin CLI 網路錯誤維持既有處理。
- **Regression check:** repository-owned check 在 updater runtime／build surfaces 發現上游 App release query、Tauri updater registration、permission、config 或 dependency 時必須以非零狀態結束，並指出檔案及命中的禁用模式；合法 attribution 不得使檢查失敗。
- **Plan contract:** Phase 7 明確描述個人安裝，不列 auto-update 為完成條件；若未來需要對外發布，必須用新的 Spectra change 定義簽章、notarization、release hosting 與 update trust root。
- **Acceptance criteria:** `npm run build`、`npm run lint`、`npm run check:i18n`、新的 updater source check 與 `cargo test --manifest-path src-tauri/Cargo.toml` 全部成功；人工啟動 App 等待原本三秒檢查時段並開啟 Settings，確認沒有 App 更新通知或控制項。
- **In scope:** App binary updater 的 backend、frontend、Tauri config／capability、dependencies、i18n、回歸檢查及 Phase 7 文字。
- **Out of scope:** Artifact 更新、Git upstream、branding、Bundle ID、資料遷移與公開發布基礎設施。

## Risks / Trade-offs

- [Risk] 未來不會由 App 提醒 AgentDeck 程式碼有更新 → Mitigation：目前只有單一開發者且不對外發布，上游差異由 Git 開發流程管理。
- [Risk] 靜態檢查誤判合法 attribution → Mitigation：只掃描明確的 runtime／build surfaces，不掃描文件與 Spectra artifacts。
- [Risk] 移除 dependency 後 lockfile 留下不可達套件 → Mitigation：使用既有 package／Cargo 工具更新 lockfiles，並以 production build 與 Rust tests 驗證。
- [Risk] 後續 upstream merge 重新帶回 updater → Mitigation：將新的來源檢查納入驗收命令，讓回歸在提交前失敗。

## Migration Plan

1. 先移除 UI／context／IPC 使用點，再移除 Tauri plugin 註冊、capability、config 與 dependencies。
2. 重新產生或更新 lockfiles，執行完整驗收與人工啟動檢查。
3. 既有安裝不需資料 migration；新版啟動後單純不再提供 App 更新功能。
4. rollback 可還原本 change 的程式碼與 manifests，但在沒有自有發布策略前不得重新啟用上游 updater。

## Open Questions

無；AgentDeck 不對外發布，因此完整移除 App updater 已定案。
