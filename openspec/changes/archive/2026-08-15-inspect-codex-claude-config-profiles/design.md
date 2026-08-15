## Context

這是 `plan.md` Phase 6 的第一個 change。AgentDeck 已有 Project records、Tauri IPC、Hooks 的 JSON／TOML 處理經驗，以及 React 專用管理頁，但尚未提供 Config Profiles runtime capability。Codex 使用 user 與 project `config.toml`；Claude Code 使用 user、project 與 local `settings.json`。設定檔可能同時含安全偏好、命令、路徑、環境變數與 credentials，因此「成功解析整份文件」不代表整份資料可傳給 frontend。

本 change 的利害關係人是要確認多層設定的 AgentDeck 使用者，以及後續 profile apply change 的實作者。主要限制如下：

- 只允許固定 home path 與 AgentDeck 已登錄 Project root 衍生的路徑，caller 不得指定檔案路徑。
- 只使用既有 `serde_json`、`toml_edit`、`sha2` 與標準函式庫，不新增 production dependency。
- inventory 是唯讀 snapshot，不是 Codex／Claude Code runtime 的完整解析器；CLI flags、environment、managed policy 與未掃描來源可能另行覆寫。
- 任何未知或敏感內容即使可被 parser 讀到，也不得進入 serializable DTO、log 或 diagnostic message。
- 本 change 保留上游跨平台行為；home resolution 與 path join 使用既有平台抽象，不新增 macOS-only API。

官方設定參考：

- Codex config reference：`https://developers.openai.com/codex/config-reference/`
- Claude Code settings：`https://docs.anthropic.com/en/docs/claude-code/settings`

## Goals / Non-Goals

**Goals:**

- 安全列出固定來源的存在、解析與 fingerprint 狀態。
- 只正規化明確 allowlist 內的非敏感 scalar settings。
- 顯示受支援來源集合內的 precedence、override 與 diff，並清楚標示這不是完整 runtime effective config。
- 讓單一缺檔或壞檔以 typed status 隔離，不使其他來源失效。
- 提供真正唯讀的 Config Profiles 頁面，不製造空白或誤導性的 mutation control。

**Non-Goals:**

- 不支援 managed policy、CLI flags、environment、Codex named profile file 或 caller-supplied settings file。
- 不匯入未知 key、permission rule、Hook、MCP、Plugin、command、path list、environment map 或 credential value。
- 不建立 ConfigProfile Artifact、database migration、profile persistence、project assignment 或 deployment state。
- 不進行 write preview、write、repair、format、backup、restore、secret storage 或 Git backup。
- 不宣稱 inventory 等同 Codex／Claude Code 實際 session 的完整 effective settings。

## Decisions

### 固定來源與 Project identity

新增單一 Tauri command `get_config_profile_inventory`，輸入只接受 optional `project_id` 與 Agent／scope filters；backend 從 `SkillStore` 查出 project record 後才組合固定相對路徑。user source 由 backend 的 home resolution 組合，frontend 不得提交 home、root 或 source path。

Codex sources 固定為 `~/.codex/config.toml` 與 `<registered-project>/.codex/config.toml`。Claude sources 固定為 `~/.claude/settings.json`、`<registered-project>/.claude/settings.json`、`<registered-project>/.claude/settings.local.json`。未提供 project 時只回 user sources。不存在的固定檔案回 `missing` source status，不是整體錯誤；未知 `project_id` 回 command-level `project_not_found`，不掃描任何 path。symbolic link source 不跟隨並回 `unsupported_symlink`，避免已登錄專案內的連結變成任意檔案讀取入口。

替代方案是讓 frontend 提交 path 或遞迴搜尋設定檔；兩者都擴大 filesystem authority，且與 Phase 6 固定來源邊界不符，因此拒絕。

### Bounded parser 與來源隔離

每個 source 以標準函式庫 metadata 與 bounded read 獨立處理，檔案上限固定為 1 MiB。Codex 交給 `toml_edit` parse，Claude 交給 `serde_json` parse；不執行 CLI、不做 shell expansion，也不解析設定中引用的其他檔案。每個 source 的狀態固定為 `missing`、`available`、`unreadable`、`too_large`、`unsupported_symlink` 或 `invalid_format`。

diagnostic 只包含 stable code、Agent、scope、project ID 與 source ID；parser 原始錯誤、行內容、OS error detail 與 raw bytes 不跨過 DTO boundary。成功來源以 raw bytes 計算 SHA-256 fingerprint，讓 UI 能識別 refresh 間的外部修改，但本 change 不利用 fingerprint 寫檔。

替代方案是重用 CLI 輸出；兩個 Agent 沒有共同、固定且保證不洩漏敏感內容的 config inventory command，因此直接 bounded parse 的 authority 更小、失敗模式也更可測。

### 明確 scalar allowlist 與 DTO boundary

backend 只抽取下列 exact key 與指定 scalar type：

- Codex：`model`、`model_reasoning_effort`、`model_verbosity`、`approval_policy`（只接受 string form）、`sandbox_mode`、`web_search`、`service_tier`、`personality`。
- Claude Code：`model`、`alwaysThinkingEnabled`、`autoUpdatesChannel`、`cleanupPeriodDays`、`fastMode`、`permissions.defaultMode`。

共同 DTO `ConfigProfileInventoryDto` 包含 `sources`、`settings` 與 `diagnostics`。`ConfigSourceDto` 包含 opaque source ID、Agent、scope、optional project ID、固定 display path、status、optional fingerprint 與 `has_unexposed_fields`。`ConfigSettingDto` 包含 Agent、canonical key、native key、typed display value（`string`、`boolean` 或 `integer`）、scope、source ID、optional project ID 與 resolution。DTO 不包含 raw document、generic JSON/TOML value、unknown key name、parser message 或任意 nested object。

allowlist key 的型別錯誤只產生 `invalid_allowed_value` diagnostic，內容只指出 allowlisted key，不附原值；同來源其他合法 allowlist keys 仍可顯示。`env`、API key helpers、provider credentials、MCP、Hooks、permission allow／deny rules、commands、paths與所有未知 key 一律不抽取。`has_unexposed_fields` 只表示來源另有未顯示內容，不回傳 key name、數量或值。

替代方案是先 serialize 一份 redacted document；redaction 容易因上游新增 key 或巢狀結構漏掉秘密。正向 allowlist 從資料形狀上排除未知內容，失敗時也維持封閉。

### Supported-source precedence 與唯讀 diff

resolution 只在「本 change 掃描的受支援來源」內計算。Codex 順序是 user 後接 project；Claude Code 是 user、project、local。相同 Agent／project／canonical key 的最高支援 scope 標為 `observed_active`，被覆蓋項標為 `observed_overridden`，無 project context 的 user 值標為 `observed_active`。不同 Agent 的相似設定不互相覆蓋，也不強行轉換成相同語意。

UI 必須同時顯示來源 scope 與固定說明：CLI flags、environment、managed policy、project trust 與未掃描來源可能改變實際 runtime value。Codex project config 因 trust 才載入，因此其值標記為 `project_candidate`，不宣稱必然生效；它仍可與 user value 做差異比較。

diff 由正規化後的 allowlisted typed values 計算，狀態為 `same`、`added`、`changed` 或 `removed`，不對 raw document 做文字 diff。替代方案是顯示完整 TOML／JSON diff；那會把未知與敏感內容送往 frontend，因此拒絕。

### 唯讀頁面與不變的 persistence

新增 `/config-profiles` route 與 sidebar entry，頁面提供 Agent、scope、project filters、refresh、source diagnostics、setting table 與 normalized diff。頁面不出現 create、edit、assign、apply、backup 或 restore controls；空狀態須區分沒有已登錄 project、固定 source missing 與 source invalid。

command 不取得 write handle，不寫入 Library、SQLite、source config、Application Support、log 或系統安全儲存。沒有 migration；rollback 是移除 route、command 與新增模組，既有資料不需轉換。外接 Library offline 不阻擋唯讀固定 config source inspection，且 command 不觸發 Library sync 或 delete。

## Implementation Contract

### Observable behavior

使用者開啟 Config Profiles 後，可選 Agent、已登錄 project 與 scope，看到固定來源的狀態、allowlisted 設定值、來源層級、受支援來源內的 override／diff，以及不完整 runtime resolution 的明確提示。任一來源缺失、過大、無法讀取、為 symlink 或格式錯誤時只影響該來源；其他來源仍顯示。

### Interface / data shape

- Tauri command：`get_config_profile_inventory(request)`。
- request：optional `project_id`、Agent filter、scope filter；不得含 path、home、cwd、environment 或 raw config。
- response：`ConfigProfileInventoryDto { sources, settings, diagnostics }`，其 DTO 欄位依「明確 scalar allowlist 與 DTO boundary」決策。
- stable diagnostic codes：`project_not_found`、`unreadable`、`too_large`、`unsupported_symlink`、`invalid_format`、`invalid_allowed_value`。
- frontend wrapper 與 TypeScript types 必須與 serde snake_case JSON shape 一致。

### Failure modes

- unknown project ID：command-level typed error，零來源讀取。
- missing source：source status `missing`，不是 fatal error。
- unreadable、too large、symlink 或 invalid format：該 source 無 settings，回 stable status／diagnostic，其他 source 繼續。
- 單一 allowlisted value 型別錯誤：跳過該 key，回 `invalid_allowed_value`；同 source 其他合法 keys 繼續。
- refresh 期間檔案改變：下一次 response fingerprint 與值反映新 snapshot；本 change 不快取、不寫回、不宣告 conflict resolution。

### Acceptance criteria

- Rust unit／integration tests 使用 temporary home 與 project records，覆蓋所有固定來源、precedence、diff、size bound、symlink、invalid TOML／JSON、unknown project、敏感／未知內容排除與零寫入。
- serialization tests 證明 raw document、unknown key、secret value、parser／OS error detail 不出現在 response JSON。
- frontend production build、lint、i18n check 與新增 Config Profiles UI static contract 全部通過。
- 人工 GUI 驗證 valid、missing、invalid 與 refresh 四條唯讀流程；測試前後 snapshot 證明真實 `~/.codex`、`~/.claude`、Library 與 Application Support 沒有寫入。
- `spectra analyze inspect-codex-claude-config-profiles --json` 無 Critical／Warning，`spectra validate inspect-codex-claude-config-profiles` 通過。

### Scope boundaries

實作只包含固定來源 discovery、bounded parse、allowlist normalization、source diagnostics、supported-source precedence／diff 與唯讀 UI。profile persistence、mutation、assignment、backup、restore、secret storage、managed policy、CLI／environment resolution 與任意來源路徑都不在範圍內。

預期受影響檔案完整清單：

- `src-tauri/src/core/config_profile_inventory.rs`（新增）
- `src-tauri/src/commands/config_profile_inventory.rs`（新增）
- `src-tauri/src/core/mod.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/views/ConfigProfiles.tsx`（新增）
- `src/App.tsx`
- `src/lib/tauri.ts`
- `src/components/Sidebar.tsx`
- `src/i18n/en.json`
- `src/i18n/zh-TW.json`
- `scripts/check-config-profiles-ui.mjs`（新增）
- `package.json`
- `openspec/specs/config-profile-inspection/spec.md`（archive 時新增）
- `openspec/specs/product-board-interface/spec.md`（archive 時修改）

## Risks / Trade-offs

- [allowlist 只涵蓋小部分設定] → UI 明示未顯示內容，後續擴充必須經 spec 與安全檢查，不以 generic redaction 擴大。
- [畫面中的 observed value 與實際 session 不同] → 顯示 supported-source 限制，Codex project 值標為 candidate，不解析 CLI、environment 或 managed policy。
- [上游設定格式新增或改型別] → exact key＋exact type fail closed，單一 key diagnostic 不拖垮來源。
- [惡意 project 以大檔或 symlink 探測其他檔案] → 1 MiB bound、拒絕 symlink、project ID backend lookup、固定 path join。
- [fingerprint 暴露檔案變動] → 只對成功解析的固定設定來源回傳 SHA-256，不回 raw bytes，也不接受任意 path。
- [不追蹤 Library offline] → 本 capability 不依賴 Library，也不觸發同步；這是縮小耦合的刻意取捨。
