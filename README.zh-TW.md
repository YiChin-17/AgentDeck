<p align="center">
  <img src="assets/icon.png" width="80" />
</p>

<h1 align="center">AgentDeck</h1>

<p align="center">
  一個 app，管理你所有 coding 工具的 AI agent skills。
</p>

## AgentDeck fork 方向

AgentDeck 是 [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager) 的 fork，依 MIT License 保留授權。上游基準與驗證證據記錄在 [`BASELINE.md`](BASELINE.md)。

AgentDeck 在上游的 skill 管理基礎上延伸，讓你從單一桌面 app 管理 Codex 與 Claude Code 的 Skills、Plugins、Hooks 與 Config Profiles。macOS 是第一個目標平台。上游既有的跨平台行為維持不變，除非後續有明確變更該相容性邊界的規格。

桌面版建置以 `AgentDeck` 為產品名稱，`io.github.yichin17.agentdeck` 為固定 Bundle ID。這次產品識別變更刻意保留既有的 `.skills-manager` 儲存目錄、`skills-manager.db`、備份協定、`skills-manager-git-backup` Keychain 服務、本機偏好設定鍵值，以及 `skills-manager-cli` 指令介面，讓既有資料與自動化流程繼續運作。

沿用的 GitHub OAuth 整合在 GitHub 授權頁面上可能仍顯示為 `skills-manager`；要撤銷存取權時請以該實際名稱為準。在 macOS 上，Bundle ID 變更可能讓新舊兩個 app 同時存在。啟動 AgentDeck 前請先關閉舊的 app。確認 AgentDeck 能開啟既有 Library 與備份設定後，你可以自行移除 `Skills Manager.app`。AgentDeck 不會刪除舊 app，也不會刪除使用者資料。

<p align="center">
  🎬 <a href="https://www.youtube.com/watch?v=wfbCrfNASVU">影片介紹 (YouTube)</a>
  &nbsp;·&nbsp;
  <a href="https://www.bilibili.com/video/BV1845F6REUu/">影片介紹 (Bilibili)</a>
</p>

<p align="center">
  <a href="./README.md">English</a>
</p>

<p align="center">
  <img src="assets/demo/library.png" width="800" alt="AgentDeck Library" />
</p>

<p align="center"><strong>安裝 Skills — Marketplace</strong></p>
<p align="center"><img src="assets/demo/install-skills.png" width="800" alt="Install Skills Marketplace" /></p>

<p align="center"><strong>Global Workspace</strong></p>
<p align="center"><img src="assets/demo/global-workspace.png" width="800" alt="Global Workspace" /></p>

<p align="center"><strong>Agent Workspace</strong></p>
<p align="center"><img src="assets/demo/agent-workspace.png" width="800" alt="Agent Workspace" /></p>

<p align="center"><strong>Project Workspace</strong></p>
<p align="center"><img src="assets/demo/project-workspace.png" width="800" alt="Project Workspace" /></p>

<p align="center"><strong>備份與多裝置同步</strong></p>
<p align="center"><img src="assets/demo/backup.png" width="800" alt="Backup and multi-device sync" /></p>

<p align="center"><strong>Settings</strong></p>
<p align="center"><img src="assets/demo/settings.png" width="800" alt="Settings" /></p>

## 功能

- **統一的 skill library** — 從 Git repo、本機資料夾、`.zip` / `.skill` 壓縮檔，或 [skills.sh](https://skills.sh) marketplace 安裝 skills。全部集中存放在同一個 central repo，預設是 `~/.skills-manager`，可在 **Settings** 中自訂。
- **Marketplace 與 AI 搜尋** — 瀏覽 marketplace 上的熱門 skills、用關鍵字搜尋，或填入 API key 啟用 SkillsMP AI 搜尋。
- **Presets** — 把 skills 分組成具名的 preset。在任一 workspace 點一下 preset pill，就能為目前的 agent 範圍一次啟用或停用該組所有 skills。側邊欄會列出所有 presets 方便快速取用。
- **Global Workspace** — 每個 agent 有自己的頁面，列出它 global 資料夾中的每一個 skill，包含不是透過 AgentDeck 安裝的，所以畫面永遠反映 agent 實際看到的內容。可以逐一 agent 新增或移除 skills，也可以用 All Agents 總覽一次管理所有已安裝的 agent。
- **Project Workspaces** — 檢視與管理支援的 agent 在專案內的 skill 資料夾，與你的 central library 比對，並雙向同步變更。支援巢狀 skill 目錄，匯出時可逐一指派 agent。
- **Linked Workspaces** — 把任意目錄指定為 skills root，適合放在預設 agent 路徑之外的 skills。以獨立 workspace 管理，不參與 global preset 同步。
- **多工具同步** — 一鍵透過 symlink 或複製，把 skills 同步到任何支援的工具。每張 skill card 會為每個啟用的 agent 顯示一個 icon badge，點 badge 就能直接在卡片上為該 agent 安裝或移除該 skill，badge 也會即時反映同步狀態。
- **Add from Library 面板** — 在任一 workspace 點 **+ Add Skills** 開啟統一的挑選面板：搜尋你的 central library、用常駐的 chips 切換目標 agents（支援全選／清除），一次批次加入多個 skills。
- **批次操作** — 多選 skills 後批次啟用／停用、匯出或刪除。Project Workspaces 也支援對專案內 skills 批次啟用／停用。
- **Skill 標籤與篩選** — 為 skills 加標籤、用標籤把相似的 skills 歸類，並依來源或標籤篩選，其中 **Untagged** pill 可以快速找出還沒標記的 skills。
- **更新追蹤** — 檢查 Git 來源 skills 的上游更新；本機來源的則重新匯入。
- **Skill 預覽與來源檢視** — 直接在 app 內閱讀 `SKILL.md` / `README.md`、檢視來源 metadata，並比對本機內容與上游版本的差異。
- **自訂工具** — 加入你自己的 agent／工具與對應的 skills 目錄，或覆寫任何內建工具的預設路徑。
- **備份與多裝置同步** — 一次登入即可連接私有 GitHub repository（或任何 Git remote），app 會自動備份你的 library 並讓所有已連線裝置保持同步。合併是 skill 導向的，一台機器上的重新命名能與另一台上的內容編輯乾淨地合併；真正的衝突永遠不會卡住流程：在你選擇保留本機／使用遠端／兩者都留之前，本機版本原封不動。快照版本隨時可還原。
- **Activity log 與 Export Logs** — 安裝／移除／更新／同步的操作都會記錄在本機。用 **Settings → Export Logs** 把近期 logs 與活動紀錄打包成單一 zip，方便回報問題。
- **彈性的 app 設定** — repo 路徑、同步模式、佈景主題、文字大小、語言、常駐列行為、proxy、Git remote、Skill 更新偏好，以及 agents 在各處顯示的排序，全部集中在同一個地方設定。

## 核心概念

<p align="center">
  <img src="assets/diagram-concept-map.png" width="640" alt="Concept map: Library, Preset, Global Workspace, Project Workspace, Agent" />
</p>

- **Preset 是可重複使用的 skill 群組** — preset 是一組具名的 skills。在任一 workspace 啟用某個 preset，就會把它的所有 skills 加到選定的 agents；停用則移除。套用 preset 是一次性的複製，不是持續同步。
- **Global Workspace 管理各 agent 的全域 skills** — 每個已安裝的 agent 都有自己的 global skills 資料夾（例如 Claude Code 的 `~/.claude/skills/`）。每個 agent 頁面會列出該資料夾裡的所有內容，即使不是透過 AgentDeck 安裝的也一樣，你可以新增、移除或納管；All Agents 總覽則一次管理所有 agent。
- **Project Workspace 是專案內的 skill 集合** — project workspace 管理存在於特定專案內的 skills（例如 `<project>/.claude/skills/`）。在這裡加入的 skills 只對該專案生效。
- **標籤用來分組與篩選** — 用標籤標記相似的 skills，再依標籤篩選，快速找到你要的子集合。
- **批次控制在任何地方都能用** — 在任一 workspace 多選 skills 進行批次操作。

## 快速上手

1. 從本機資料夾、Git repository、壓縮檔或 marketplace 安裝 skills。如果你有 SkillsMP API key，也可以開啟 AI 搜尋。
2. 從側邊欄開啟 **Global Workspace** 並選一個 agent（例如 Claude Code）。
3. 點 **Preset** pill 為該 agent 啟用該組 skills，或用 **+ Add Skills** 從 library 挑選並直接切換目標 agents。已啟用的 preset 會顯示 ✓；部分安裝則顯示數量 badge。
4. 要管理專案內的 skills，開啟 **Project Workspace**，使用同樣的 preset pills 或帶有多 agent 目標選擇器的 **+ Add Skills** 面板。
5. 在 **Settings** 中設定 agent 路徑、自訂工具、佈景主題、語言、proxy 與 Git 偏好。
6. 需要歷史版本或多機同步的話，從側邊欄開啟 **Backup** 並點 **Sign in with GitHub**，之後備份與跨裝置同步都會自動執行。

## 備份與多裝置同步

側邊欄的 **Backup** 頁面會把你的 skill library 以 Git repository 保存版本。單一裝置可獲得帶有可還原快照的版本化備份；多台裝置連到同一個 repository 則會自動彼此同步。remote 始終是一般的 Git repository，你可以在任何地方 `git clone`，沒有鎖定。

### 連線

- **Sign in with GitHub**（建議）：8 位數的 device-flow 登入會為你建立一個私有的 `skills-manager-backup` repository。token 存在作業系統的 keychain，絕不寫進檔案或 repo 設定。
- **進階**：在 **Settings → Git Sync Configuration** 貼上任何 Git URL（HTTPS + PAT、SSH、自架皆可）。
- 在 library 為空的新機器上，首次啟動會詢問：**從頭開始，還是從備份還原？**

### 同步怎麼運作

- **自動**：你停止編輯後幾分鐘，本機變更會在背景 commit 並 push；其他裝置推上來的更新會被合併並自動推回。**Back Up Now** 隨時可立即執行一次，而且歷史中的每筆備份都會標示是哪台裝置做的。
- **skill 導向的合併**：變更以 skill 為單位合併，而不是以文字行為單位，所以在一台機器上重新命名 skill，能與另一台上編輯它的內容乾淨地合併。
- **衝突不會卡住也不會覆蓋**：如果同一個 skill 同時在兩台裝置上被編輯，其他東西照常同步，而該 skill 保留你的本機版本並出現在 **Needs attention**（Library 中它的卡片上也會有 badge）。選擇**保留本機／使用遠端／兩者都留**，套用任何選擇之前都會先做一份安全快照，所以每個決定都可以還原。
- **快照與還原**：手動備份會產生快照版本；開啟 Backup 頁面的歷史即可還原任一個。還原前會先把目前狀態存成它自己的快照。

### 備份包含什麼

Skills、標籤、presets 與各 agent 的 skill 開關會被備份。祕密資訊（API keys、tokens、proxy 設定）與機器專屬的設定永遠不會離開這台機器。超過 100 MB 的 skills 會留在本機並自動排除在備份之外（Backup 頁面上會標示）。SQLite 資料庫不放進 Git，它存的是可以從 skill 檔案重建的 metadata。

### 中斷連線

Backup 頁面提供三個層級：**中斷這台機器的連線**（其他裝置與遠端資料不受影響）、**撤銷 GitHub 授權**，或**完全刪除遠端備份**（走 GitHub 自己的輸入名稱確認流程）。

## 支援的工具

開箱支援 52 個 agents，包含：

Claude Code · Codex · Cursor · GitHub Copilot · Gemini CLI · OpenCode · OpenClaw · Hermes Agent · OpenHands · Cline · Goose · Windsurf · Continue · Grok · Antigravity · Qwen Code · Crush · Kilo Code · Roo Code · Amp · Kiro CLI · Droid · TRAE IDE · Warp · Qoder · CodeBuddy

**Settings** 會列出全部，並把在你機器上偵測到的排在前面。你也可以在那裡加入自訂工具，並以同樣方式管理它們的 skills。

## App 內說明

**Settings** 中的 **Help** 按鈕對應目前的產品流程：建議的工作流程、presets、skill 安裝、Library（含 Untagged 篩選與逐卡刪除）、Global Workspace 與 **+ Add Skills** 面板、帶有多 agent 目標選擇器的 Project Workspaces、備份與多裝置同步，以及環境層級的設定（含用於回報問題的 Export Logs）。它就是這份快速上手指南的 app 內版本。

## 技術棧

| 層級 | 技術 |
|-------|------|
| 前端 | React 19, TypeScript, Vite, Tailwind CSS |
| 桌面 | Tauri 2 |
| 後端 | Rust |
| 儲存 | SQLite (`rusqlite`) |
| i18n | react-i18next |

## 開始開發

### 前置需求

- Node.js 18+
- Rust toolchain
- 你的作業系統對應的 [Tauri 前置需求](https://v2.tauri.app/start/prerequisites/)

### 開發

```bash
npm install
npm run tauri:dev
```

### CLI

這個 repository 內含一個對 agent 友善的 CLI，建立在桌面 app 所使用的同一套 Rust shared core 上。CLI 與桌面 app 走的是同一個 SQLite 資料庫、同一個 central library 與同一套同步引擎。

```bash
# Repository / library 總覽
npm run cli -- repo status
npm run cli -- skills list
npm run cli -- skills show db

# 安裝 skills（預設只進 library，不會同步到 agents）
npm run cli -- skills install ./my-skill                       # 本機路徑
npm run cli -- skills install https://github.com/foo/bar.git   # git URL
npm run cli -- skills install vercel-labs/agent-skills@react-best-practices  # skills.sh
npm run cli -- skills install foo/bar --sync                   # 加入 active preset 並同步到 agents

# 從上游更新／檢查（git skills 重新 clone，本機 skills 重新匯入來源）
npm run cli -- skills update --all
npm run cli -- skills check --all

# 搜尋 skills.sh marketplace（不需要 API key）
npm run cli -- skills search react --limit 5

# 移除（必須加 --yes；可用 --dry-run）
npm run cli -- skills remove <ref> --dry-run
npm run cli -- skills remove <ref> --yes

# 透過變更 preset 成員來啟用／停用 skills
npm run cli -- presets add-skill <preset> <ref>
npm run cli -- presets remove-skill <preset> <ref>

# 把 active preset 同步到已啟用的 agents
npm run cli -- skills sync --dry-run
npm run cli -- skills sync --tool claude_code

# 納管 agent 目錄中既有的 skills（例如 ~/.claude/skills/）
npm run cli -- skills adopt ~/.claude/skills --dry-run
npm run cli -- skills adopt ~/.claude/skills

# 標籤
npm run cli -- skills tag add <ref> web frontend
npm run cli -- skills tag list

# Presets
npm run cli -- presets list
npm run cli -- presets preview Default
npm run cli -- presets apply Default
npm run cli -- presets add-skill <preset> <skill>
npm run cli -- presets remove-skill <preset> <skill>

# 匯出單一 skill 到任意目錄（一次性複製，不納入管理）
npm run cli -- skills export db --dest ~/.claude/skills/db

# Git 管理的 skills repo
npm run cli -- git status
npm run cli -- git pull
npm run cli -- git commit -m "chore: update skills"
```

可用的指令群組：
- `repo` — 檢視或變更設定的基準目錄
- `tools` — 列出偵測到的工具目標與路徑
- `skills` — 管理 central library 中的 skills（`list / show / install / update / check / remove / enable / disable / sync / search / adopt / tag / export`）
- `presets` — 列出 presets、預覽／套用、在 preset 中加入或移除 skills
- `git` — 操作以 git 管理的 `skills/` repository（`clone`、`pull`、`push`、`commit`、`versions`、`restore`）

額外的 flags：
- `--skills-root <path>` — 直接對某個已 clone／已匯出的 skills repo 操作，而不是本機 app 的預設目錄。manager 的狀態（DB、presets、cache、logs）會放在 `~/.skills-manager/external/<name>-<hash>/`，依 skills root 的正規化路徑分目錄隔離，外部 checkout 本身保持乾淨。
- `--json` — 供腳本／agent 使用的機器可讀輸出

```bash
npm run -s cli -- --skills-root /path/to/my-skills --json skills list
```

#### 把執行檔安裝到 PATH

如果 agent 或腳本要直接呼叫 `skills-manager-cli`（不透過 `npm run`），需要先把執行檔放到 PATH 上：

```bash
npm run cli:install
# 等同於：
# cargo install --path src-tauri --bin skills-manager-cli --locked --force
```

執行檔會裝到 `~/.cargo/bin/skills-manager-cli`。程式碼更新後再跑一次即可刷新。

#### 與桌面 app 同時使用

CLI 與桌面 app 共用同一個 SQLite 資料庫。SQLite 會安全地序列化寫入，但執行中的 app 不會在 CLI 變更狀態時自動刷新記憶體中的快取，所以在 `presets apply`、`git pull` 或其他 CLI 寫入操作之後，請重新啟動 app 或在 app 內手動刷新。

### 建置

```bash
npm run tauri:build
npm run cli:build
```

## macOS 發布（目前未啟用）

**目前沒有公開的 AgentDeck release。** AgentDeck 是為擁有者個人使用而維護的，因此不提供任何下載，這個 repository 中的 release workflow 也從未對真實的簽章憑證執行過。安裝 AgentDeck 就是自己建置，做法見下一節。

[docs/macos-distribution.md](docs/macos-distribution.md) 作為備而不用的資料保留：它記錄的是，如果發布行為未來被另一個變更明確授權，一個已簽章的磁碟映像會如何被識別與驗證——架構、SHA-256 checksum、Developer ID 簽章、公證、Gatekeeper 與撤回。

## 個人安裝（macOS）

AgentDeck 靠你自己建置來安裝。個人的本機建置**沒有應用程式自動更新**、沒有公開的 release 託管、沒有 Developer ID 簽章，也**沒有公證保證**，而且不從任何其他地方繼承簽章、公證或託管的信任。你裝的是你所 checkout 的那個 commit 的個人本機建置，以下每一句話都只適用於該建置。

### 1. 從已 commit 的 lockfile 建置

```bash
npm ci
npm run tauri:build
```

`npm ci` 會安裝 `package-lock.json` 中的確切相依版本，Rust 端則依 `src-tauri/Cargo.lock` 建置，所以同一個 commit 會產生同一個應用程式。建置會寫出：

- `src-tauri/target/release/bundle/macos/AgentDeck.app` — 應用程式
- `src-tauri/target/release/bundle/dmg/AgentDeck_<version>_<arch>.dmg` — 同一次建置的 macOS 安裝檔

兩者都不納入 Git 追蹤。用這個指令驗證你剛建好的東西：

```bash
npm run check:personal-installation
```

它會確認 bundle 名稱、`io.github.yichin17.agentdeck` Bundle ID、版本、執行檔、安裝檔，以及不存在任何應用程式更新機制，並印出一行摘要。

### 2. 安裝應用程式

開啟 `.dmg` 並把 `AgentDeck.app` 拖進 `/Applications`，或自己複製過去。個人的 `~/Applications` 資料夾也一樣可行。

因為這個建置沒有用 Developer ID 憑證簽章，macOS 會要求你核准一次。請核准這個 app 本身——在 Finder 中右鍵點它選 **打開**，或在第一次被擋下後開啟 **系統設定 → 隱私權與安全性** 點 **仍要打開**。這是逐一應用程式的核准；不要為了執行這個建置而關閉 Gatekeeper 或任何其他系統安全檢查。

### 3. 首次啟動與既有資料

首次啟動時，AgentDeck 會開啟這台機器上既有的資料。它沿用既有的 `.skills-manager` 儲存目錄、`skills-manager.db` SQLite 資料庫、presets、已登錄的 Projects、部署紀錄、Git 備份 metadata、`skills-manager-git-backup` Keychain 項目與本機偏好設定鍵值，全部維持原本的名稱。schema 遷移就地執行且可重試；不會有任何東西被重新命名、搬移、複製或刪除。

如果你之前也用過 **Skills Manager.app**，請在啟動 AgentDeck 前先關掉它——兩者是共用同一份資料的不同應用程式。

### 4. Library 位置與重新連上離線的 Library

內部 Library 存在應用程式自己的資料目錄中。外部 Library 則留在你設定的位置，包含可卸除式磁碟或網路磁碟區。

當設定的外部 Library 無法存取時，AgentDeck 會為該 Library 顯示 **Library Offline** 並且什麼都不改：它不會建立替代的 Library、不會把部署重新指向別處，也不會記錄刪除。重新接上磁碟區後，用 **Retry** 動作把同一個 Library 帶回來。

### 5. 備份與還原

用 **Backup** 頁面連接 Git remote，然後按 **Back Up Now** 立即做一次版本化備份。要還原的話，開啟備份歷史並挑一個快照——目前狀態會先被存成它自己的快照，所以還原是可以復原的。在 library 為空的機器上，首次啟動會提供從既有備份還原的選項，而不是從頭開始。完整行為見 [備份與多裝置同步](#備份與多裝置同步)。

### 6. 解除安裝

把 `AgentDeck.app` 從 `/Applications` 移除只會移除應用程式。**它不會移除你的資料。** 你的 library、資料庫、備份 metadata 與已儲存的憑證都留在原處，所以之後重新安裝新的建置時會再次接上它們。

如果你也想清掉資料，請個別移除下列項目，而且只移除你真的願意失去的：

- 家目錄中的 `.skills-manager` 儲存目錄——library 內容、presets 與部署紀錄
- 外部 Library 目錄，如果你把它設定在該儲存目錄之外
- **鑰匙圈存取** 中的 `skills-manager-git-backup` 項目——Git 備份憑證
- Git 備份的 remote 本身，如果你不再需要那份版本歷史

`skills-manager-cli` 執行檔如果你有安裝，位置在 `~/.cargo/bin/skills-manager-cli`，要另外移除。

## 疑難排解

### macOS 又要求存取 `skills-manager-git-backup` 鑰匙圈項目

個人建置的程式碼簽章在你每次重新建置時都會改變，而 macOS 把鑰匙圈存取權綁在該簽章上。安裝新的本機建置之後，第一次 Git 備份可能會要求存取 `skills-manager-git-backup` 項目的權限。為新的建置點 **一律允許** 即可。

## License

MIT
