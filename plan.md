# AgentDeck 開發計畫

最後更新：2026-08-13

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

目前狀態（2026-08-13）：進行中。第一個 Spectra change `inspect-codex-claude-hooks` 已完成並歸檔，交付 Codex／Claude Code Hooks 的唯讀 discovery、來源檢視、設定差異與 compatibility matrix。下一個 change 加入表單編輯、Agent-specific schema validation、write preview、可回復備份與 atomic write；仍不執行 Hook。

- [x] 先做讀取、檢視與 diff。
- [ ] 加入表單編輯、Agent-specific schema validation、write preview、backup 與 atomic write。
- [x] 加入 Codex／Claude compatibility matrix。

完成標準：能安全讀寫兩者 Hooks，格式錯誤時不覆蓋原檔。

### Phase 5：Plugins

- 建立 Codex 與 Claude CLI adapters。
- 解析 CLI JSON 輸出。
- 顯示 installed／available／version／scope／updates。
- 依 CLI 能力加入 install、update、remove、enable、disable。

完成標準：常用 Plugin 操作能從同一 GUI 完成，官方 cache 與登入資料不被直接改寫。

### Phase 6：Config Profiles

- 支援 Codex TOML 與 Claude JSON 的可選欄位集合。
- 建立 profile、專案指派、preview diff、backup 與 restore。
- secrets 只保留引用或交由系統安全儲存，不寫進 profile。

完成標準：能把同一組非敏感設定安全套用到多個專案，並可預覽及回復。

### Phase 7：穩定化與個人安裝

- 完成 migration、同步、衝突、offline、CLI adapter 測試。
- 測試真實 Codex 與 Claude 專案。
- 執行本機 Tauri build，驗證 `.app` 或平台安裝包可供個人安裝。
- 整理使用說明，並驗證資料備份與解除安裝方式。
- 公開 release hosting、distribution signing、notarization 與 auto-update 不列為此階段完成條件；若未來改為對外發布，必須建立新的 Spectra change，定義簽章、notarization、release hosting 與 update trust root。

## 10. 驗證策略

每一階段至少執行：

- React／TypeScript production build。
- Rust unit/integration tests。
- 資料庫 migration tests。
- 真實暫存專案的 symlink 與 copy 測試。
- 設定檔 round-trip 測試，確認未知欄位與註解沒有被意外刪除。
- 外接 Library 拔除或 unmount 的 offline 測試。
- 套用、取消與回復流程的人工 GUI 驗證。

目前已驗證的基準結果（2026-08-13）：

- 前端 production build 通過。
- Rust tests：586 passed，0 failed。
- npm 與 Rust production dependency audits 均為 0 vulnerabilities。

## 11. 已知風險

- Codex Skill 官方路徑仍可能演進，adapter 不可只硬編碼一個位置。
- 外接磁碟名稱改變會使絕對 symlink 失效。
- Plugin CLI 的功能不完全對稱，GUI 需要明確顯示不支援的操作。
- Hook command 可能執行任意程式，介面必須讓使用者看得到完整命令與來源。
- 上游 Git backup／merge 目前以 Skill 為中心，擴充 Artifact 時要處理格式版本。
- Tauri macOS sandbox 與外接資料夾權限會影響發佈策略；若啟用 sandbox，需要保存使用者授權的資料夾存取資訊。

## 12. 下一次對話的起點

下一階段是 Phase 4 的 Hooks 安全編輯與寫入，不直接加入 Hook 執行、Plugin 或 Config Profile 功能：

1. 以既有 `hook-inspection` source id 與 linked Project 邊界選定可寫來源，frontend 仍不得傳入任意 filesystem path。
2. 依 Codex／Claude Code 各自 schema 編輯 event、matcher 與 handler fields；未知欄位、JSON sibling keys、TOML 註解與排序必須在 round-trip 後保留。
3. 寫入前產生實際設定檔 diff，驗證失敗或來源在預覽後被外部修改時拒絕套用，不覆蓋較新的內容。
4. 套用時先建立可回復備份，再以同目錄暫存檔與 atomic replacement 寫入；任一步驟失敗都不得留下部分寫入或損壞原檔。
5. 明確定義 Hook Artifact identity、backup metadata 與 restore 邊界，但不得把 Hook command、prompt、URL、headers 或其他敏感內容寫入 SQLite、一般 Library、logs 或 Git backup。
6. 只在下一個 Spectra change 的 proposal、design、spec 與 tasks 通過 analyze 與 validate 後開始實作。

## 13. 尚待使用者決定

目前無。Hooks 唯讀檢視已完成；下一個 change 的範圍固定為安全編輯、驗證、預覽、backup、atomic write 與 restore，不包含 Hook 執行。
