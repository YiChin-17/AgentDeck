# 更新日誌

本專案所有顯著變更都會記錄在這個檔案中。

格式基於 [Keep a Changelog](https://keepachangelog.com/zh-TW/1.1.0/)，
版本號遵循 [語意化版本](https://semver.org/lang/zh-TW/)。

AgentDeck 的版本號從 1.0.0 開始，與上游 Skills Manager 的版本序列各自獨立。
上游到 v1.30.0 為止的歷史保存在 [`CHANGELOG-legacy.md`](CHANGELOG-legacy.md)。

## [1.0.0] - 2026-08-19

### 發布概覽
- AgentDeck 自有版本序列的第一個版本。App 現在有自己的產品識別、自己的 repository，也不再依賴上游的 release 基礎設施。
- 除了管理 Skills 之外，AgentDeck 現在能從單一桌面 app 檢視與編輯 Codex 與 Claude Code 的 Hooks、Plugins 與 Config Profiles。

### 使用者可見的更新
- **Codex 與 Claude Code Hooks** — 檢視每個 agent 實際載入的 Hook 設定，在寫入任何東西之前先看到來源、scope 與格式錯誤。編輯在 app 內完成，具備 schema 驗證、實際 diff、外部修改衝突偵測與復原路徑。
- **Plugins** — 唯讀盤點 Codex 與 Claude Code 已安裝的內容，並支援 user scope plugin 的安裝與狀態變更，過程中不碰任何一方 CLI 自己的 cache。
- **Config Profiles** — 檢視每個 Codex TOML 與 Claude Code JSON 設定目前由哪個 scope 生效，再把同一組非敏感設定套用到已登錄的專案，具備預覽、衝突保護與還原。
- **預設繁體中文** — 產品介面預設為 `zh-TW`，既有的 `zh` 偏好設定不會再讓 app 以簡體中文啟動。
- **新版 Codex skill 路徑** — `.agents/skills` 成為部署預設。既有放在 `~/.codex/skills` 的 skills 仍會出現在 Agent Workspace，不會無聲消失。
- **外部 Library 保護** — 當放在未掛載外接磁碟上的 Library 無法存取時，app 不再於掛載點建立空目錄並把它當成新的 Library，而是回報該 Library 離線並且什麼都不改。
- **移除應用程式自動更新** — App 不再查詢上游 Skills Manager 的 release，也無法安裝由上游私鑰簽署的 binary。安裝 AgentDeck 就是自己建置。
- **Settings 連結指向這裡** — Settings 中的 GitHub 與回報問題入口現在導向 AgentDeck 自己的 repository，不再指向上游。（Issue #3）
- **ssh:// Git 來源維持可用** — 合法的 `ssh://` skill 來源不會再於正規化階段被改寫成無效的 GitHub HTTPS shorthand。（Issue #2）
- **proxy 設定錯誤時 fail closed** — 當設定的 proxy 無法建立 HTTP client 時，backend 會回報失敗，而不是安靜地略過 proxy 直接連線。（Issue #5）

### 開發與治理
- **自有產品識別** — bundle、視窗、選單、語系、package metadata 與圖示都改用 AgentDeck 與 `io.github.yichin17.agentdeck` Bundle ID。既有的 `.skills-manager` 儲存目錄、`skills-manager.db`、備份協定、Keychain 服務與 CLI 介面刻意保持不變，讓既有資料繼續運作。
- **獨立的 repository** — AgentDeck 現在存在於自己的 repository，不再是上游的 fork，版本序列也從這裡重新開始。
- **Artifact 基礎** — backend 的 identity、deployment 與備份 metadata 不再只以 Skill 為根型別，Hooks、Plugins 與 Config Profiles 因此能在不污染 `SkillRecord` 的前提下承接。
- **個人安裝是受支援的路徑** — 從已 commit 的 lockfile 建置，並用 `npm run check:personal-installation` 驗證結果。macOS 發布相關資料保留但未啟用，不提供公開 release。
- **安全性 advisory 已解除** — production 相依套件稽核中發現的 `quick-xml` 與 `rkyv` advisories 皆已處理。
- **擴大 pull request 驗證範圍** — frontend、locale 與 repository contract 檢查現在會在每個 pull request 上執行，不再只在 Rust 路徑變更時觸發。（Issue #4）
