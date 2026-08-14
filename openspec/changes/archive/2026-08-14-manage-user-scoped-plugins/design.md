## Context

`inspect-codex-claude-plugins` 已提供固定唯讀命令、bounded runner、Agent-specific inventory 與 route-local Plugins 頁面，但沒有 mutation IPC。本機 contract evidence 是 Codex CLI 0.144.5 的 `plugin add`／`plugin remove` 與 Claude Code 2.1.231 的 `plugin install`／`plugin update`／`plugin uninstall`／`plugin enable`／`plugin disable`。兩個 CLI 沒有 dry-run；Claude marketplace 可宣告需要互動確認的外部命令，非 TTY 執行時要求 `-y`，而 AgentDeck 不應替使用者自動接受。

官方 CLI 擁有 Plugin cache、settings、marketplace resolution 與登入狀態。AgentDeck 只能用 fresh inventory 驗證選取的 identity，以固定參數 preview mutation，並在 CLI 完成後重新讀取 inventory 證明結果。mutation 不依賴中央 Library，因此 Library offline 不得被誤報為 Plugin failure，也不得因此繞過 Plugin 驗證。

## Goals / Non-Goals

**Goals:**

- 以 Agent-specific fixed capability matrix 提供 user-scope install、update、remove、enable 與 disable，未支援操作明確 unavailable。
- 使用 preview／apply 兩階段 contract，將 fresh inventory identity、operation、固定 argv、fingerprint、expiry 與一次性 token 綁定。
- 防止 option injection、stale apply、token replay、同時 mutation、unbounded output、shell invocation、raw diagnostic leakage 與 optimistic success。
- 對 destructive remove／uninstall 顯示可核對的 Agent、Plugin、marketplace、scope 與固定 command preview。
- 保留既有 Plugin inventory、Skill、Hook、backup 與跨平台行為，不新增依賴或 schema migration。

**Non-Goals:**

- 不支援 project、local 或 managed scope，也不接收或推導 caller-controlled cwd。
- 不模擬 Codex update／enable／disable，不執行 marketplace mutation、validation、details、eval、prune、scaffold 或 tag。
- 不傳 `-y`、`--config`、`--keep-data`、`--prune` 或 `--all`，不支援 arbitrary Plugin config。
- 不讀寫官方 cache／manifest／settings，不解析 Plugin payload，不建立 Artifact、deployment、Library copy 或 Git backup metadata。
- 不提供自動 rollback；官方 CLI mutation 成功但 inventory 無法證明結果時回報 verification failure，使用者可重新整理後決定下一步。

## Decisions

### Agent-specific fixed mutation capability matrix

共用的 frontend operation enum 是 `install`、`update`、`remove`、`enable`、`disable`。Backend 以 Agent 與 operation 查固定 table：Codex `install` 對應 `codex plugin add --json -- <plugin@marketplace>`，`remove` 對應 `codex plugin remove --json -- <plugin@marketplace>`；Claude Code 對應 `claude plugin install --scope user -- <plugin@marketplace>`、`update --scope user --`、`uninstall --scope user --`、`enable --scope user --` 與 `disable --scope user --`。所有 selector 前放置 option terminator `--`，selector 必須由 fresh inventory 中完全相同的 Agent、marketplace、plugin id 組成，且拒絕空值、control／NUL、前導 `-` 與超過 512 bytes 的 identity component。

Capability table 不含任意 executable、cwd、environment 或附加 argument。Codex 的 update／enable／disable 回 `operation_unsupported`；Claude 才提供這三項。Alternatives rejected：用 remove＋add 模擬 Codex update 會把兩個 destructive state transitions 假裝成一個官方能力；直接接受 argv 會形成任意 process execution surface。

### Fresh-inventory preview and one-time token

新增 `preview_plugin_mutation`，request 只有 `agent`、`operation`、`pluginId`、`marketplace`。Backend 先呼叫既有 collector，精確找到 identity 並檢查 operation precondition：install 需要 available 且未 installed；update／remove／enable／disable 需要 installed，enable／disable 還要有相反的 known enabled state。Claude update／remove／enable／disable 要求 inventory record scope=`user`；Claude install 接受 scope=`user` 或 scope=`unknown`（production available-only record 不含 scope 欄位，解析為 `unknown`），因為 backend 固定傳 `--scope user`，scope 由 CLI 保證而非 inventory。Codex JSON 不提供 scope，因此 Codex add／remove 的 user scope 由 fixed global CLI capability contract 保證，不把 `unknown` inventory scope推測改寫為 `user`。Preview 以 SHA-256 計算 selected item、Agent CLI version、capabilities 與 operation 的 canonical JSON fingerprint，產生 UUID v4 token，並回傳 Agent、operation、identity、固定 argv display、destructive flag、expiresAt 與 token。

`PluginMutationState` 由 Tauri manage，使用既有 Tokio synchronization、UUID、SHA-256 與 hex dependencies。Pending preview 只留在記憶體，TTL 固定 120 秒、最多 128 筆；建立第 129 筆前移除 expired entries，再移除最早到期者。Token 不寫 log、SQLite、Library、localStorage 或 Git。Alternatives rejected：由 frontend 自行保存整份 intent 不能防止竄改；只比較 UI 舊 state 不能防止 CLI 或另一個 AgentDeck process 的外部修改。

### Single-use apply with stale and concurrency gates

新增 `apply_plugin_mutation`，request 只帶 token。Backend 在 mutation gate 內原子取出並消耗 token；missing、expired 或 replayed token 均不啟動 CLI。Apply 重新取得 inventory 並重算 fingerprint，必須與 preview 完全相同；identity、scope、CLI version、capability、installed／available／version／enabled 任一改變都回 `stale_preview`。所有 Agent 的 mutation 共用一個 async gate，避免兩個 official CLI 同時改 Plugin state；唯讀 refresh 不持有此 gate。

Token 在 CLI 啟動前消耗，因此 timeout、non-zero exit 或 verification failure 都不能 replay。使用者必須重新 preview，畫面會取得最新 state。Alternatives rejected：失敗後保留 token 會讓同一確認在外部狀態改變後被重送；per-Agent parallel gate 仍可能同時寫共用 marketplace/cache resources。

### Bounded mutation runner and interactive-confirmation classification

Mutation runner 沿用 inventory runner 的 no-shell、closed stdin、piped bounded streams、10-second deadline、1,048,576-byte per-stream limit、kill／reap 與 raw-output disposal contract。Success parser 只接受 command-specific JSON；Claude 非 JSON success 只以 exit 0 作 process success，最終仍需 inventory verification。Diagnostics 只包含 Agent、operation、fixed code 與 optional numeric exit status。

固定 failure vocabulary 新增 `operation_unsupported`、`identity_not_found`、`scope_unsupported`、`precondition_failed`、`preview_expired`、`stale_preview`、`interactive_confirmation_required` 與 `verification_failed`，並保留 runner 的 `cli_missing`、`timeout`、`non_zero_exit`、`output_too_large`。Claude install／update 的 bounded stderr 只在記憶體中比對一組 fixture-pinned official non-TTY confirmation phrases；命中只回 `interactive_confirmation_required`，未命中仍回 `non_zero_exit`，原文不進 IPC 或 log。

Alternatives rejected：自動傳 `-y` 可能執行 marketplace 宣告的外部命令；把所有 non-zero 說成互動需求會隱藏真正錯誤；回傳 stderr 會洩漏 path、credential 或 Plugin payload。

### Post-mutation inventory verification

CLI process exit 0 後，backend 再收集 inventory。Install 的 target 必須成為 installed；remove 的 target 必須不存在或明確 not installed；enable／disable 必須呈現目標 known state；update 必須讓 installed version 從 preview 值改變，且當 preview 有 available version 時，新 installed version必須等於該值。若 refresh 失敗、state unknown 或 observable condition 不成立，回 `verification_failed` 並附最新 sanitized inventory，不宣告成功。

Verified success 回傳 operation、identity 與完整 fresh `PluginInventoryDto`，Plugins route 以這份 response 替換 route-local state。Alternatives rejected：只看 exit 0 會在 CLI 寫入延遲、錯誤 scope 或 schema drift 時顯示假的成功；optimistic row update 會繞過 official inventory boundary。

### Capability-gated UI and destructive confirmation

Plugins 頁面從 backend 回傳的 mutation capability matrix 和 item state 決定 controls，不用 Agent 名稱在 frontend 猜能力。Claude 的非 user scope record disabled（install 例外：接受 unknown scope，因為 production available-only record 無 scope 欄位）、需要 known enablement 但狀態 unknown、或不符合 precondition 的操作 disabled 並顯示原因；Codex add／remove 使用backend宣告的fixed user-scope capability，即使唯讀 DTO 的 scope 是 `unknown` 也不在frontend改寫該欄位。Click 先取得 preview；install／update／enable／disable 顯示固定 preview summary，remove／uninstall 額外使用明確 destructive dialog，dialog 的 Agent、Plugin id、marketplace、scope 與 command display 都直接來自同一 preview response。

Apply 只送 token。成功時使用 response inventory；failure 時顯示 localized fixed diagnostic 並 refresh。頁面不保存 token 到 AppContext 或 localStorage，unmount 時只丟棄 frontend token reference，backend token靠 TTL 清理。Alternatives rejected：直接在 row button 呼叫 mutation 沒有 review boundary；frontend capability constants 容易與 CLI adapter drift。

## Implementation Contract

- **Backend interfaces:** `preview_plugin_mutation(request)` 接受 `PluginMutationPreviewRequest { agent, operation, pluginId, marketplace }`；`apply_plugin_mutation(request)` 接受 `PluginMutationApplyRequest { token }`。Preview response 包含 `token`, `expiresAt`, `agent`, `operation`, `pluginId`, `marketplace`, fixed `scope: user`, `argvDisplay`, `destructive`, `baseFingerprint`。Apply success 包含 verified `inventory`，failure 只含 fixed diagnostic fields。
- **Capability matrix:** Codex = install／remove；Claude Code = install／update／remove／enable／disable。所有 production argv 由 enum table 建立，selector 前有 `--`，scope 永遠 backend-owned `user`。
- **Preview validity:** fresh inventory identity、precondition、CLI version 與 capabilities 都進 fingerprint；Claude update／remove／enable／disable 要求 record scope=`user`；Claude install 接受 scope=`user` 或 `unknown`（available-only record 無 scope 欄位）。Codex scope由fixed add／remove contract定義且inventory的 `unknown` 原樣保留；TTL 120 秒，pending 上限 128，token single-use。Missing／expired／replayed／stale token不得 spawn child。
- **Process bounds:** no shell、stdin null、10-second timeout、stdout／stderr各 1,048,576 bytes、timeout／overflow kill and reap。禁止 argv `-y`、`--config`、`--keep-data`、`--prune`、`--all` 與任何 caller-controlled process setting。
- **Observable completion:** exit 0 只是中間狀態；fresh inventory 必須符合 operation-specific postcondition才回 success。Update 無 observable version change、enablement unknown 或 refresh failed 都回 `verification_failed`。
- **Persistence and secrets:** mutation command不接收 store／Library path，不新增 database schema，不直接讀寫 official files；captured streams、token、fingerprint、argv 與 Plugin payload不寫入 log、SQLite、Library、Git 或 localStorage。
- **Acceptance:** Rust tests固定 capability table、selector validation、option terminator、token TTL／capacity／replay、stale fingerprint、single mutation gate、interactive phrase classification、exact output boundaries、timeout/reap、Agent isolation與每個 postcondition；frontend contract固定 preview-first、token-only apply、capability gating、destructive confirmation、latest inventory replacement、locale parity與 mutation controls不出現在 unsupported records。完整 locked Rust suite、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:plugin-mutations`、既有 `npm run check:plugins-ui`及`git diff --check`均須 exit 0。
- **In scope:** Codex與Claude Code user-scope Plugin mutation、preview、apply、verification與UI confirmation。
- **Out of scope:** project／local／managed scope、arbitrary config、marketplace mutation、validation／details／eval／prune、direct cache access、persistent Plugin model與rollback automation。

## Risks / Trade-offs

- [CLI help or output changes] → Keep Agent-specific fixed tables and fixtures, gate by version/capability, return typed unsupported or verification failure instead of guessing.
- [Marketplace command requires user confirmation] → Keep stdin closed and never pass `-y`; classify only fixture-pinned non-TTY phrases and require the user to use the official interactive CLI for that case.
- [External mutation between preview and apply] → Recollect inventory under the mutation gate and reject a mismatched fingerprint.
- [CLI exits 0 before state becomes observable] → Perform one immediate bounded refresh and report verification failure rather than retrying or claiming success; the user can refresh explicitly.
- [Destructive remove has no rollback] → Require preview-based destructive confirmation and state this limitation; official CLI remains the sole state owner.
- [Token accumulation] → Use a 120-second TTL and 128-entry cap with deterministic eviction; never persist tokens.

## Migration Plan

1. Add mutation DTOs、state、fixed table、preview／apply commands and tests without changing database schema.
2. Register managed in-memory state and Tauri commands, then add typed frontend wrappers and UI confirmation.
3. Run all acceptance commands and manual fake-CLI flows before enabling controls.
4. Rollback removes mutation controls、commands、state and modules; existing read-only inventory and official CLI Plugin state remain intact.

## Open Questions

None. Project／local scope and explicit acceptance of marketplace-declared external commands require separate changes with their own trust and cwd contracts.
