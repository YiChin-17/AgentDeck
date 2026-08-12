## Context

Phase 3 已提供 `ArtifactKind::Hook`，但沒有 Hook detail table、scanner、commands 或 UI。Phase 4 的第一個切片只盤點 Agent 實際載入的設定：Codex 可在 user／project config layer 使用 `hooks.json` 或 `config.toml` inline `[hooks]`；Claude Code 可在 user、project 與 project-local `settings.json` 使用 `hooks`。兩者都採 event → matcher group → handler 三層結構，但 event 集合、handler type 與來源合併規則不相同。

目前 App 已有 Projects、固定 Sidebar、Tauri invoke API、雙語字串與 `DocumentDiffViewer`。`DocumentDiffViewer` 使用 O(n²) line diff，因此輸入大小必須受限。現有 Rust dependencies 可解析 JSON，但沒有 TOML parser；Codex inline hooks 需要 `toml_edit`，也能保留下一個寫入 change 所需的 TOML 結構與註解資訊。

官方格式基準固定為 2026-08-12 的 Codex Hooks 文件與 Claude Code Hooks reference。這個 change 不嘗試把兩套 runtime 語意合併成共用執行模型。

## Goals / Non-Goals

**Goals:**

- 從固定且已知的 Codex／Claude Code user 與已關聯 project 設定位置讀取 Hook subtree，不接受 frontend 提供任意 filesystem path。
- 將來源、scope、event、matcher、handler fields 與 diagnostics 轉成穩定的唯讀 DTO；未知 event／field／handler type 仍可見。
- 單一來源缺失或格式錯誤時，其他來源仍正常顯示。
- 提供同一 Agent 兩個來源的 canonical Hook subtree diff，並限制輸入大小避免 UI 卡住。
- 提供由固定 registry 產生的 Codex／Claude Code event 與 handler compatibility matrix。
- 在 Sidebar 提供 Hooks 頁面，支援 Agent、scope、event、source status 篩選與 Inspector。

**Non-Goals:**

- 不新增 Hook detail table、Artifact row、deployment row、schema migration 或 Library metadata。
- 不寫回、建立、刪除、enable／disable 或執行任何 Hook，也不呼叫 Agent CLI 驗證設定。
- 不讀取 managed policy、plugin bundle、Skill／agent frontmatter 或 process-local session hooks。
- 不升級 `.skills-manager` backup schema／protocol，不備份 Hook 設定或其內容。
- 不允許跨 Agent source text diff；跨 Agent 差異由 compatibility matrix 表達。

## Decisions

### 固定來源描述器與 project id 邊界

新增 `hook_inspection` core module。`get_hook_inspection(project_id?: String)` 只接受 optional project id；backend 以 `SkillStore::get_project_by_id` 解析已關聯 project root，並自行組合固定來源：

- Codex user：home 下的 `.codex/hooks.json` 與 `.codex/config.toml`。
- Codex project：project root 下的 `.codex/hooks.json` 與 `.codex/config.toml`。
- Claude Code user：home 下的 `.claude/settings.json`。
- Claude Code project：project root 下的 `.claude/settings.json` 與 `.claude/settings.local.json`。

每個來源使用穩定 enum-backed id，例如 `codex:user:hooks-json`；frontend 不傳入或回傳任意 path 來要求另一輪讀檔。未提供 project id 時只列 user sources；未知 project id 回傳 `invalid_project`，不 fallback 到 process current directory。

未採用遞迴掃描 home 或 project，因為 managed、plugin 與 nested component hooks 不在本 change 範圍，掃描也會擴大敏感檔案讀取面。

### 來源隔離 parser 與 canonical Hook fragment

JSON 使用既有 `serde_json`，TOML 新增 `toml_edit`。parser 只抽取 `hooks` subtree：Claude Code settings 的其他 sibling keys、Codex config 的其他 tables 不得進入 DTO、diff、log 或 diagnostics。每個來源獨立產生 `missing`、`valid`、`invalid` 或 `too_large` status；一個來源失敗不短路整批結果。

有效 subtree 轉成兩種輸出：

- `HookEntryDto`：保存 source id、agent、scope、event、matcher、group index、handler index、handler type 與 ordered display fields。未知 event、handler type 或 field 保留原值並標記 `unknown`，不丟棄也不推定支援。
- `HookSourceDto.canonical_text`：只含 Hook subtree 的 deterministic pretty representation。JSON object keys 依 parser 的穩定輸出排列；TOML 使用 `toml_edit` subtree representation。來源格式不同不互相比較。

不回傳完整 settings/config 文件，避免把與 Hook 無關的 token、provider credential 或 environment 設定送到 frontend。Hook command、prompt、URL 與 handler 自訂欄位是使用者要求檢視的資料，可在本機 UI 顯示，但不得寫 log、SQLite、Library、Git backup 或 localStorage。

### 限制讀取與 diff 成本

單一 config file 上限為 1 MiB；超過時標記 `too_large` 且不 parse。canonical Hook fragment 超過 256 KiB 或 4,000 lines 時仍可列出已解析 entries，但 `diff_available=false`，frontend 不把內容交給 O(n²) `DocumentDiffViewer`。空 Hook subtree 的 canonical text 是空字串，與另一個空來源比較顯示無差異。

未新增另一套 diff library；現有 component 已符合唯讀 side-by-side 需求，backend 大小界線能避免其複雜度失控。

### 文件快照驅動的 compatibility registry

在 core module 以 typed constants 固定 `CompatibilityRowDto` registry，來源註記為 2026-08-12 官方文件快照。event registry 至少完整列出該快照公開的 Codex events 與 Claude Code events；handler registry 固定 Codex `command`，以及 Claude Code `command`、`http`、`mcp_tool`、`prompt`、`agent`。

每個 cell 是 `supported`、`unsupported` 或 `unknown`，並可帶 agent-specific note。discovery 遇到 registry 外的 event 或 handler 時加入 `unknown` inspection marker，但不動態把它加入 supported matrix。更新 registry 必須透過後續程式修改與 fixture test，不從網路 runtime 抓文件。

未將同名 event 視為同語意：matrix 呈現名稱與 support level，Inspector 仍顯示原始 Agent/source；例如相同的 `PreToolUse` 不會轉換 output contract。

### 唯讀 Tauri DTO 與 Hooks UI

新增 command module，回傳 `HookInspectionDto`：

- `sources: HookSourceDto[]`
- `entries: HookEntryDto[]`
- `compatibility: CompatibilityRowDto[]`
- `selected_project_id: String | null`
- `generated_at: i64`

`HookSourceDto` 包含 source id、agent、scope、format、display path、status、sanitized diagnostic、entry count、canonical text 與 diff availability。diagnostic 只包含來源 id、錯誤種類與 parser message；不得包含完整檔案內容。

新增 `/hooks` route。頁面預設顯示兩個 Agent 的 user sources；使用者選擇既有 Project 後才加入 project sources。list filters 只在已載入 DTO 上運作。選取 entry 開啟 `HookInspector`，完整顯示 event、matcher、handler type、source 與 fields；選取同一 Agent 的兩個 `diff_available` sources 時重用 `DocumentDiffViewer`。跨 Agent、相同來源、invalid／too-large source 會停用 Compare 並顯示具體原因。

不把 Hooks 放入全域 `AppContext`，因為目前只有 `/hooks` 一個 caller；route 自己管理 load/filter state，避免為單一 consumer 新增全域抽象。

### 靜態 UI 契約與雙語邊界

新增 `check:hooks-ui` script，固定 `/hooks` route、Sidebar entry、Tauri command 名稱、Agent／scope／status filters、Inspector 與 diff component wiring。所有使用者可見字串同時加入 `src/i18n/en.json` 與 `src/i18n/zh-TW.json`，並由既有 `check:i18n` 驗證 key parity。

不在本 change 引入 frontend test framework；production TypeScript build、ESLint、靜態契約 script 與 Rust parser／DTO tests 已覆蓋這個唯讀切片的可驗證邊界。

## Implementation Contract

**Observable behavior**

- 開啟 Hooks 頁面時，user scope 的 Codex／Claude Code Hook sources 會個別顯示為 missing、valid、invalid 或 too large；missing 是正常空狀態，不顯示全頁錯誤。
- 選擇已關聯 Project 後，頁面加入該 root 的 Codex／Claude Code project sources；清除選擇後不保留 project Hook 內容。
- 有效來源的每個 matcher group／handler 都可在 Inspector 看見原始 Agent、scope、source、event、matcher、type 與已知或未知 fields。
- 一個壞 JSON／TOML source 只在該來源顯示 diagnostic，其他 valid sources 與 compatibility matrix 仍可用。
- 同一 Agent 的兩個可比較來源可顯示 canonical Hook subtree diff；不支援的 pair 顯示明確原因且不執行 diff。
- 所有操作都是 read-only；觀察前後設定檔 bytes、SQLite、Library tree 與 Git working tree 均不變。

**Interface and data shape**

- Tauri command 固定為 `get_hook_inspection`，input 只有 `projectId: string | null`。
- Agent persisted values 固定為 `codex` 與 `claude_code`；scope values 固定為 `user`、`project` 與 `project_local`；format values 固定為 `json` 與 `toml`。
- source status 固定為 `missing`、`valid`、`invalid`、`too_large`；compatibility support 固定為 `supported`、`unsupported`、`unknown`。
- Hook entries 以 source id、event、matcher group index 與 handler index 組合出一次 response 內穩定的 UI id；此 id 不寫入 database，也不承諾跨外部檔案重排保持不變。
- DTO 不包含完整 config/settings document、非 Hook sibling keys、database identity、deployment state 或可執行 action。

**Failure modes**

- home directory 無法解析時，user sources 回傳 sanitized invalid diagnostics；不改用 process current directory。
- project id 不存在時，整個 command 回傳 typed `invalid_project` error；不讀取任意 path。
- permission denied、invalid UTF-8、JSON／TOML syntax error、Hook subtree shape error與 size limit 各自映射到來源 diagnostic；不得 panic。
- 未知 event／handler／field 保留並標記 unknown；不得 drop、fallback 成共同 event 或阻止其他 entries。
- frontend request race 以最後一次 project selection 為準；較舊 response 不得覆蓋較新選擇。

**Acceptance criteria**

- Rust fixtures 覆蓋 2 個 Codex formats、3 個 Claude Code layers、missing、invalid、permission／UTF-8 error、1 MiB limit、unknown values、multi-handler ordering、project id validation 與非 Hook sibling exclusion。
- DTO serialization test 斷言固定 enum strings、canonical text／diff flags，並證明含 token 的非 Hook sibling 不出現在 serialized response 或 diagnostic。
- compatibility registry fixture 斷言官方快照中的 Codex／Claude Code event 與 handler rows、三態 support，以及未知值不升級成 supported。
- frontend static check 斷言 route、Sidebar、filters、Inspector、Compare guard 與 `DocumentDiffViewer` wiring；`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:hooks-ui` 全部 exit 0。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` 與 `git diff --check` exit 0；manual fixture 比較讀取前後 config bytes、database、Library tree 與 Git status 無變化。

**Scope boundaries**

- In scope：固定 user／linked-project sources、JSON／TOML Hook subtree parser、typed DTO、sanitized diagnostics、compatibility registry、Hooks route、filters、Inspector 與 bounded source diff。
- Out of scope：Hook persistence、editor、write preview、atomic replacement、backup／restore、deployment、execution、CLI validation、managed／plugin／component hooks與跨 Agent config conversion。

## Risks / Trade-offs

- [Risk] 官方 event／handler 集合在實作後演進 → registry 帶快照日期，未知項目仍顯示 unknown；後續更新需修改 constants 與 fixtures，不靜默推定。
- [Risk] settings/config 含敏感 sibling data → parser 只抽 Hook subtree，response 與 logs 禁止完整文件；tests 放入 sentinel token 並斷言不洩漏。
- [Risk] Hook command 本身含 credential → 唯讀 UI 需顯示使用者實際設定，因此只在當次 response 與記憶體呈現，不持久化、不記錄、不備份。
- [Risk] project symlink 或權限讓讀取指向預期外位置 → 只從已關聯 project root 組合固定相對位置並採 read-only；UI 顯示實際來源 path，任何錯誤按來源隔離。
- [Risk] 大型來源讓 parse 或 line diff 卡住 → 1 MiB read limit與 256 KiB／4,000-line diff limit fail closed，entries 與 diagnostic 仍可用。
- [Risk] 第一段不建立 Hook Artifact identity → 明確以 ephemeral source entry 為 UI identity；持久 identity、editing 與 backup 一併留給下一個 proposal，避免先承諾不穩定 key。

## Migration Plan

1. 先加入 parser／source descriptor／registry fixtures，再實作 core module與 command DTO。
2. 接上 `/hooks` route、Sidebar、filters、Inspector、bounded diff與雙語字串。
3. 跑完整 Rust／frontend／靜態契約驗證，並用 temporary HOME／project fixtures證明 read-only。
4. 此 change 沒有 database 或 config migration；rollback 是移除 route、command與新 dependency，外部設定與 Library 不需回復。

## Open Questions

無。Hook persistence、編輯、atomic write與 backup protocol 由 Phase 4 的下一個 Spectra change決定。
