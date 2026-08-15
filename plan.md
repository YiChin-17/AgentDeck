# AgentDeck 開發計畫

最後更新：2026-08-15

## 1. 專案摘要

AgentDeck 是一個 macOS 桌面 GUI，用來集中管理 Codex 與 Claude Code 的 Skills、Plugins、Hooks 與 Config Profiles。

主要問題：

- 同一個 Skill 經常需要複製到多個專案與不同 Agent 的資料夾。
- 個人層級安裝太多 Skill 後，只能逐專案停用，不容易掌握實際啟用狀態。
- 有些 Skill 只適用單一專案，有些則需要在多個專案間搬移。
- Codex 與 Claude Code 使用不同的設定位置與格式，目前缺少統一管理介面。

核心操作方式：建立中央 Library，使用者在類似 Trello 的 Board 上，把項目指派給 Codex、Claude 或兩者。每個專案保留自己的啟用組合，實際部署可使用 symlink 或 copy。

## 2. 已確認的技術決策

- 以 `xingkongliang/skills-manager` 為上游專案建立 fork，不從零重寫。
- 沿用 Rust + Tauri 2 + React + TypeScript + SQLite 架構。
- 第一階段以 macOS 為主要平台，但避免破壞上游既有的跨平台能力。
- 專案原始碼位置：`/Volumes/Work_Space/Project/AgentDeck`。
- 外接磁碟為 APFS、PCI-Express SSD，適合存放原始碼與編譯產物。
- App 的 SQLite 資料庫預設留在 Mac 內部磁碟，避免外接磁碟未掛載時無法啟動。
- Skill Library 預設放內部磁碟，並允許使用者改選外接磁碟位置。
- GUI 直接安全地編輯 JSON/TOML 設定；Plugin 的安裝、更新、移除與驗證優先呼叫官方 CLI。
- 不把 Plugin、Hook、Config Profile 強行塞進既有的 Skill 資料模型；它們共用上層 Artifact 概念，但保留各自的資料與部署規則。
- 桌面產品名稱固定為 `AgentDeck`，Bundle ID 固定為 `io.github.yichin17.agentdeck`；npm／Cargo desktop package 與預設 desktop binary 使用 `agentdeck`。
- 產品改名不改持久協議：`.skills-manager`、`skills-manager.db`、Git backup metadata／refs／trailers、`skills-manager-git-backup` Keychain service 與既有 localStorage keys 保持不變。
- `skills-manager-cli` 是既有 Skill automation contract，暫不改名；GitHub OAuth 撤銷指引也保留 GitHub 實際顯示的外部名稱 `skills-manager`。

## 3. 儲存位置規劃

### 原始碼

```text
/Volumes/Work_Space/Project/AgentDeck/
```

### App 內部資料

```text
~/.skills-manager/
├── skills-manager.db
├── skills/
├── scenarios/
├── cache/
└── logs/
```

Library 位置設定沿用 `~/Library/Application Support/skills-manager/repo-config.json`。Bundle ID 只影響 Tauri 管理的 logs／WebView container 等非核心狀態，不搬移或重建上述 Library、SQLite 與 backup 資料。

### 中央 Library

預設位置：

```text
~/.skills-manager/
```

可由使用者改選的位置，例如：

```text
/Volumes/Work_Space/AgentDeck-Library/
```

外接 Library 未掛載時必須顯示 `Library Offline`，暫停同步與刪除操作，不能把失聯檔案判定為使用者已刪除。

## 4. 目標路徑與相容策略

### Codex

- 使用者設定：`~/.codex/config.toml`
- 專案設定：`<repo>/.codex/config.toml`
- 新版官方 Skill 位置：`~/.agents/skills`、`<repo>/.agents/skills`
- 舊版或既有 Skill 位置：`~/.codex/skills`、`<repo>/.codex/skills`
- Plugin marketplace：`~/.agents/plugins/marketplace.json` 或 `<repo>/.agents/plugins/marketplace.json`
- Plugin cache 屬於工具管理的內部資料，不由 AgentDeck 直接修改。

Codex 部署預設使用 `.agents/skills`，同時掃描並辨識 `.codex/skills`。相同 Skill 不可重複顯示，使用者可在進階設定覆寫目標路徑。

### Claude Code

- 使用者設定：`~/.claude/settings.json`
- 專案設定：`<repo>/.claude/settings.json`
- 專案本機設定：`<repo>/.claude/settings.local.json`
- 使用者 Skills：`~/.claude/skills`
- 專案 Skills：`<repo>/.claude/skills`
- Plugins：`~/.claude/plugins`

Claude Code 已支援 symlink Skill 目錄，但同步前仍要確認來源、目標與衝突狀態。

## 5. 介面方向

### Board

主要畫面採四欄：

1. Library
2. Codex
3. Claude
4. Both

卡片可拖曳，也可從右側 Inspector 使用勾選框指派。Board 顯示的是同一筆 Artifact 的部署狀態，不因出現在多欄而建立重複資料。

### Description 層級

- 卡片：最多兩行 `display_summary`，方便快速掃描。
- 右側 Inspector：完整 `description` 與 `when_to_use`。
- 深入頁：完整 `SKILL.md`、frontmatter、來源檔案與變更差異。

Skill 真正的 `description` 可能影響 Agent 判斷何時載入 Skill，因此不能為了畫面長度自動截斷或改寫。`display_summary` 是 AgentDeck 自己的顯示欄位，不回寫取代原始描述。

### 主要側欄

- Projects
- Library
- Skills
- Plugins
- Hooks
- Config Profiles
- Settings

## 6. 資料模型方向

既有上游模型可繼續使用：

- Skills
- Skill targets
- Projects
- Scenarios／Presets
- Tags
- Git backup metadata

新增上層 Artifact 概念：

```text
Artifact
├── Skill
├── Plugin
├── Hook
└── ConfigProfile
```

部署狀態至少需要記錄：

- Artifact
- Project 或 global scope
- Agent：Codex／Claude
- Enabled
- Deployment mode：symlink／copy／CLI-managed
- Source path
- Target path
- Last synced hash/time
- Conflict 或 offline 狀態

各類 Artifact 應使用個別細節表或服務，不把不同格式混進單一 SkillRecord。

## 7. 安全同步原則

- 套用前提供 Preview Diff。
- JSON/TOML 只修改使用者選定的欄位，保留未知欄位。
- Codex TOML 使用能保留註解與排列的編輯方式，例如 Rust `toml_edit`。
- 寫入採暫存檔加原子替換，並先建立可回復備份。
- 移除 symlink 前確認它確實由 AgentDeck 管理。
- 來源磁碟 offline 時禁止自動清除目標或資料庫紀錄。
- 不把 token、登入資訊或其他 secrets 匯入一般 Library 與 Git 備份。
- 發現外部修改時先顯示衝突，不直接覆蓋。

## 8. Plugin 與 Hook 管理策略

### Plugins

Plugin 不只是 JSON manifest，還可能包含 Skills、Hooks、MCP servers、scripts、dependencies、cache 與登入狀態。

- GUI 統一顯示可用、已安裝、版本、scope 與更新狀態。
- Codex 優先使用 `codex plugin ... --json` 能力。
- Claude 優先使用 `claude plugin ... --json` 能力。
- 安裝、更新、移除、enable／disable 依各 CLI 實際支援範圍執行。
- AgentDeck 不直接修改官方 Plugin cache。

### Hooks

- GUI 提供事件、條件、命令、Agent 與 scope 編輯。
- Codex 與 Claude 的 Hook schema/event 不相同，必須各自驗證。
- JSON 只保存 Hook 設定；實際 script 或 executable 仍是外部檔案，需要檢查路徑與執行權限。
- 套用前顯示實際將寫入的設定差異。

## 9. 開發階段

### Phase 0：建立 Fork 與基準線

目前狀態（2026-08-12）：已完成。Fork 與 upstream 來源、MIT License、起點 commit 與 tag 均已記錄；基準 build、tests 與 production dependency audits 均已通過。`quick-xml`、`rkyv`、`rustls-webpki` 與 `tar` 的 production advisories 已完成相容修復，相關 Spectra changes 已歸檔。

- 從 `xingkongliang/skills-manager` 建立本機 repo。
- 設定自己的 `origin` 與原作者的 `upstream`。
- 保存 MIT License 與必要 attribution。
- 記錄 fork 起點 commit。
- 安裝依賴，確認前端 build 與 Rust tests 維持通過。
- 檢查並處理 production dependency security advisories。

完成標準：乾淨的 Git working tree、基準測試通過、README 清楚標示來源與產品方向。

### Phase 1：Codex 路徑與 Library 基礎

目前狀態（2026-08-12）：已完成。Codex 現代／legacy Skill 路徑的部署、掃描、去重與 override 已實作；外接 Library offline 時的可用性偵測、寫入封鎖與安全恢復也已完成，相關 Spectra changes 已歸檔。

- 調整 Codex adapter，以 `.agents/skills` 為新部署預設。
- 保留 `.codex/skills` 掃描與 legacy 支援。
- 新增路徑去重與使用者 override。
- 新增 Library offline 防護。

完成標準：能正確掃描兩種 Codex 路徑，且外接 Library 離線不會觸發刪除。

### Phase 2：AgentDeck Board 與 Description

目前狀態（2026-08-12）：已完成。Library／Codex／Claude／Both 四欄 Board、drag-and-drop、固定 Inspector、兩行 summary、Board／List 狀態共用與 Skill 包操作已實作，相關 Spectra change 已歸檔。

- 改造主畫面為 Library／Codex／Claude／Both 四欄。
- 使用專案既有 drag-and-drop 依賴。
- 新增固定右側 Inspector。
- 卡片顯示兩行 summary。
- Inspector 顯示完整 description、when-to-use、targets、deployment mode 與 diff。
- 由既有每 Skill／每 Agent 啟用資料推導四欄狀態。

完成標準：使用者可用拖曳或勾選改變部署目標，資料不會因跨欄產生重複副本。

### Phase 3：Artifact 基礎模型

目前狀態（2026-08-12）：已完成。Artifact identity、typed detail boundary、canonical deployment storage 與 schema v8 無損 migration 已實作；既有 Skill、offline Library 與 Git backup protocol 2 相容性均已驗證，相關 Spectra change 已歸檔。

- 新增 Artifact type 與 deployment target 資料模型。
- 建立資料庫 migration。
- 保留現有 Skills、Scenarios、Tags 與 Git backup 相容性。
- 規劃 backup protocol 版本升級與舊資料遷移。

完成標準：舊資料可無損升級，Skill 功能與上游版本行為一致。

### Phase 4：Hooks

目前狀態（2026-08-13）：已完成。`inspect-codex-claude-hooks` 與 `edit-codex-claude-hooks` 均已歸檔；Codex／Claude Code Hooks 現在具備固定來源 discovery、唯讀檢視、compatibility matrix、Agent-specific 表單驗證、write preview、外部修改衝突偵測、可回復備份、atomic write 與 restore，且不會執行 Hook。

- [x] 先做讀取、檢視與 diff。
- [x] 加入表單編輯、Agent-specific schema validation、write preview、backup 與 atomic write。
- [x] 加入 Codex／Claude compatibility matrix。

完成標準：能安全讀寫兩者 Hooks，格式錯誤時不覆蓋原檔。

### Phase 5：Plugins

目前狀態（2026-08-14）：已完成。`inspect-codex-claude-plugins` 與 `manage-user-scoped-plugins` 均已歸檔；Codex／Claude Code 固定參數 CLI adapters、bounded inventory、Agent-specific normalization、Plugins 頁面與 user scope mutation preview／apply 已實作。完整 Rust suite 762 tests、frontend build、lint、i18n、Plugins UI contract、mutation contract 與人工 GUI success／stale／cancel 流程均通過。

- [x] 建立 Codex 與 Claude 唯讀 CLI capability adapters，限制 executable 與參數組合。
- [x] 解析 CLI JSON 輸出並正規化 installed／available／version／scope／marketplace／enabled／update 狀態。
- [x] 顯示 Agent、狀態、scope 與 marketplace filters，以及來源診斷與 Plugin details。
- [x] 依 CLI 實際能力加入 install、update、remove、enable、disable。

已完成 change 的邊界：

- Codex 只提供 user scope 的 add／remove；不把重新 add 推測成 update，也不提供 CLI 未宣告的 enable／disable。
- Claude Code 提供 user scope 的 install／update／uninstall／enable／disable，所有 scope 都顯式傳入 `user`。
- GUI 先產生 Agent、operation、Plugin identity、scope 與固定 argv 的 preview；使用者確認相同 preview token 後才執行，外部 inventory 變更會使 token 失效。
- 不傳 `-y`、`--config`、`--keep-data`、`--prune`、`--all`、任意 cwd／environment 或 caller-controlled arguments；需要互動確認的 marketplace command 回報 typed diagnostic，不自動接受外部命令。
- project／local scope mutation、marketplace mutation、validation、details、eval、Plugin payload inspection 與 persistent Plugin Artifact 仍不在此 change。

完成標準：常用 Plugin 操作能從同一 GUI 完成，官方 cache 與登入資料不被直接改寫。

### Phase 6：Config Profiles

目前狀態（2026-08-15）：已完成。`inspect-codex-claude-config-profiles` 與 `manage-codex-claude-config-profiles` 均已歸檔；固定 user／project／local 來源 discovery、1 MiB bounded parse、非敏感 allowlist normalization、typed diagnostics、supported-source precedence／diff、typed profile CRUD、已登錄 Project／Agent assignment、project-scope preview／apply、owner-private recovery 與 conflict-safe restore 均已完成。完整 Rust suite 894 tests、frontend build、lint、i18n、兩個 Config Profiles UI contracts、fake-IPC success／cancel／stale／double-confirm 操作、Spectra analyze／validate 均通過。

- [x] 先以固定、受支援的路徑唯讀取得 Codex TOML 與 Claude JSON 設定，保留來源 scope、解析錯誤與未知欄位，不讀取或顯示 secret 值。
- [x] 建立 Agent-specific 可選欄位集合與 canonical normalization，顯示 user／project／local 有效值、來源與唯讀 diff。
- [x] 建立只含 allowlist 非敏感 scalar 的 reusable profile 與已登錄專案指派。
- [x] 建立 project-scope write preview、外部修改衝突偵測、backup、atomic write 與 restore。
- secrets 只保留引用或交由系統安全儲存，不寫進 profile。

唯讀 inspection change 的已完成邊界：

- change 名稱固定為 `inspect-codex-claude-config-profiles`，只建立唯讀 discovery、解析、正規化、來源診斷與 Config Profiles 頁面。
- Codex 只讀 `~/.codex/config.toml` 與已登錄專案的 `<repo>/.codex/config.toml`；Claude Code 只讀 `~/.claude/settings.json`、`<repo>/.claude/settings.json` 與 `<repo>/.claude/settings.local.json`。
- 只顯示明確 allowlist 的非敏感欄位；secret、credential、token、API key、環境變數值與未知欄位內容不進入 frontend DTO 或 log。
- 保留 Agent、scope、來源檔案、存在狀態、解析狀態與檔案 fingerprint；無效 TOML／JSON 以 typed diagnostic 隔離，不讓單一來源使整頁失敗。
- 本 change 不建立持久 Config Profile、不寫回設定檔、不套用專案指派、不建立 backup／restore，也不修改系統安全儲存。

Management change 的已完成邊界：

- change 名稱固定為 `manage-codex-claude-config-profiles`，建立 ConfigProfile Artifact detail、reusable profile CRUD、已登錄專案指派與安全的 project-scope apply／restore。
- profile 只持久化既有 inspection allowlist 內的 typed scalar；secret、credential、token、API key、環境變數、permission rules、command、path 與 unknown key 不得進入 profile、SQLite、frontend mutation DTO、log 或 Git backup metadata。
- assignment 只可引用 AgentDeck 已登錄 project ID；Codex 寫入 `<repo>/.codex/config.toml`，Claude Code 寫入 `<repo>/.claude/settings.json`。本 change 不修改 user sources 或 Claude `<repo>/.claude/settings.local.json`。
- preview 只顯示 allowlisted typed diff，並綁定 target source fingerprint、profile revision、project ID、Agent 與 exact mutation；apply 必須重新讀取並在任何 revision／fingerprint 不符時回 `stale_preview`。
- TOML 使用既有 `toml_edit` 保留未知欄位、註解與排列；JSON 只改選定 allowlist keys 並保留其他 key/value。寫入前建立 owner-private recovery backup，使用同目錄 staged file、sync 與 atomic replace；任何一步失敗不得留下部分設定或已提交 assignment state。
- restore 只使用該 ConfigProfile deployment 的最新有效 recovery point，先 preview、再以 current fingerprint 防衝突，成功前先把目前 bytes／absence 保存為新的 recovery point。
- Config Profiles 頁面新增 create／edit／delete、project assignment、preview／confirm apply 與 restore；不提供任意 path、user／local scope mutation、secret storage、background auto-apply 或跨專案單鍵批次 mutation。

完成標準：能把同一組非敏感設定安全套用到多個專案，並可預覽及回復。

### Phase 7：穩定化與個人安裝

目前狀態（2026-08-15）：規劃中。下一個 change 固定為 `stabilize-personal-installation`，只處理個人安裝前的 regression 修補、macOS 本機 bundle 產出／smoke verification，以及安裝、資料備份與解除安裝文件；不建立公開更新或發佈信任鏈。

- 完成 migration、同步、衝突、offline、CLI adapter 測試。
- 測試真實 Codex 與 Claude 專案。
- 執行本機 Tauri build，驗證 `.app` 或平台安裝包可供個人安裝。
- 整理使用說明，並驗證資料備份與解除安裝方式。
- 公開 release hosting、distribution signing、notarization 與 auto-update 不列為此階段完成條件；若未來改為對外發布，必須建立新的 Spectra change，定義簽章、notarization、release hosting 與 update trust root。

下一個 change 的邊界：

- 保留既有 `.skills-manager` Library／SQLite、Git backup protocol、Keychain service、localStorage keys 與 `skills-manager-cli` automation contract；migration smoke test 必須從既有資料快照啟動並證明資料不被重建或遺失。
- 以 temporary registered Codex／Claude Projects 驗證 Skill sync、conflict、Library offline、Hook、Plugin 與 Config Profile 的既有主要流程；只修正會阻擋個人安裝或使既有 contract 失敗的 regression，不新增新的管理能力。
- 執行鎖定依賴的 frontend／Rust／contract suites與 `npm run tauri:build`，記錄 `.app` 與 macOS installer artifact 的實際路徑、Bundle ID、版本、啟動結果及首次啟動資料相容性。
- 更新 `README.md` 的本機個人安裝、首次啟動、既有資料沿用、Library Offline、備份／還原與解除安裝說明；文件不得宣稱 AgentDeck 具備 app auto-update、公開 release hosting、distribution signing 或 notarization。
- 公開 release、對外 distribution、Developer ID signing、notarization、release hosting、update manifest、update public key與 app auto-update 全部 out of scope。

## 10. 驗證策略

每一階段至少執行：

- React／TypeScript production build。
- Rust unit/integration tests。
- 資料庫 migration tests。
- 真實暫存專案的 symlink 與 copy 測試。
- 設定檔 round-trip 測試，確認未知欄位與註解沒有被意外刪除。
- 外接 Library 拔除或 unmount 的 offline 測試。
- 套用、取消與回復流程的人工 GUI 驗證。

目前已驗證的基準結果（2026-08-15）：

- 前端 production build 通過。
- Rust tests：894 passed，0 failed。
- npm 與 Rust production dependency audits 均為 0 vulnerabilities。

## 11. 已知風險

- Codex Skill 官方路徑仍可能演進，adapter 不可只硬編碼一個位置。
- 外接磁碟名稱改變會使絕對 symlink 失效。
- Plugin CLI 的功能不完全對稱，GUI 需要明確顯示不支援的操作。
- Hook command 可能執行任意程式，介面必須讓使用者看得到完整命令與來源。
- 上游 Git backup／merge 目前以 Skill 為中心，擴充 Artifact 時要處理格式版本。
- Tauri macOS sandbox 與外接資料夾權限會影響發佈策略；若啟用 sandbox，需要保存使用者授權的資料夾存取資訊。

## 12. 下一次對話的起點

下一階段是 Phase 7 的個人安裝穩定化，不擴張到公開 release、distribution signing、notarization、hosting 或 app auto-update：

1. change 名稱為 `stabilize-personal-installation`；先盤點並重跑 migration、sync／conflict、Library offline、CLI adapters、Hooks、Plugins 與 Config Profiles 的既有 regression contracts，只修正阻擋個人安裝的失敗。
2. 使用 temporary registered Codex／Claude Projects 做 end-to-end smoke verification，證明 Skill deployment、Plugin CLI preview、Hook／Config Profile mutation與 recovery 都維持既有固定 authority、preview-first與 rollback 邊界。
3. 使用既有 Library／SQLite fixture 啟動 packaged app，確認 schema migration、`.skills-manager` 資料、Git backup metadata、Keychain service、localStorage keys 與 `skills-manager-cli` contract 都沿用，不建立平行資料命名空間。
4. 執行 `npm run tauri:build`，驗證產出的 AgentDeck `.app` 與 macOS installer artifact 的 Bundle ID、版本、可啟動性、主要頁面與 Library Online／Offline 狀態；記錄可重複的 artifact path 與 smoke checklist。
5. 更新 `README.md`，提供本機個人安裝、首次啟動、既有 Skills Manager 資料沿用、Library 位置／offline、Git backup、還原與解除安裝步驟；明確標示此 build 沒有 app auto-update 或公開 distribution 保證。
6. 完整 frontend／Rust／Node contract suites、dependency audits、`git diff --check`、Spectra analyze／validate 與 packaged-app smoke 全部通過後 park；本輪只建立 proposal、design、spec 與 tasks，不直接實作。

## 13. 尚待使用者決定

目前無。Phase 6 已完成；下一個 change 的範圍固定為 `stabilize-personal-installation` 的 regression、packaged-app smoke與個人安裝文件，不包含公開 release、distribution signing、notarization、hosting、update manifest／trust root或 app auto-update。
