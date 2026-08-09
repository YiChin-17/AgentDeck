## Context

`adopt-modern-codex-skill-paths` 已把 Codex global primary 改為 `~/.agents/skills`，並把 `~/.codex/skills` 保留在 `ToolAdapter.additional_scan_dirs`。core scanner 與 project workspace 已讀取 additional roots，但 Agent Skills 畫面的 `get_global_local_skills` 仍只呼叫 `adapter.skills_dir()`；文件讀取、匯入、pull 與 delete commands 也只接受 `agent + skill_relative_path`，再把 relative path 接回 primary root。

直接把 additional roots 加進列表會產生兩個問題。第一，modern 與 legacy 有不同內容的同名 Skill 時，兩筆資料會有相同 `relative_path`，前端 key、loading state 與 backend lookup 都會碰撞。第二，additional roots 是 discovery-only 來源，不能讓既有 pull 或 delete action 寫入 legacy 目錄。本 change 因此需要一起調整 backend 掃描、IPC identity、安全驗證及 Agent Skills UI。

## Goals / Non-Goals

**Goals:**

- Agent Skills 畫面發現 primary 與 additional global Skill roots，並維持 primary-first precedence。
- canonical root alias 與相同內容副本不重複顯示，內容不同的同名 Skill 保持可見。
- 每筆 action 以列表回傳的實際 `path` 唯一定位，backend 不信任 client path，操作前以 fresh scan 驗證。
- additional root Skill 標記為 read-only；允許文件檢視與匯入中心，但不直接修改或刪除來源。
- primary Skill 的既有 actions 與 global target 行為不變；override 指向 legacy 目錄時仍視為 writable primary。
- UI 清楚顯示 read-only 來源，並以 path 區分同名 rows 與 action state。

**Non-Goals:**

- 不搬移、刪除或自動修正 legacy Skill。
- 不改變 project workspace、global deployment target、settings schema、database schema、sync mode 或 file watcher contract。
- 不自動合併內容衝突，也不新增衝突解決 UI。
- 不改變 Plugins、Hooks、Config Profiles 或其他 artifact flows。
- 不讓 Agent Skills client 傳入任意 filesystem path；只有 fresh scan 回傳的項目可操作。

## Decisions

### 掃描 roots 並用 precedence 去重

Agent Skills 專用掃描先讀 `adapter.skills_dir()`，再依序讀存在的 `adapter.additional_existing_scan_dirs()`。開始遍歷前以 canonical root identity 去除 alias，第一個 root 保留 precedence；primary 使用 adapter 既有 recursive 設定，additional roots 沿用 global scanner 的 flat discovery 行為。

每個掃描結果保留實際 absolute `path`、`relative_path`、content hash 與 root role。相同 agent、normalized name、enabled state 與 content hash 的結果只保留第一筆；因此 identical modern／legacy copies 顯示 primary。content hash 不同時兩筆都保留。

替代方案是只按名稱或 relative path 去重，但會隱藏內容衝突；另一方案是直接重用 project scanner 的 project-relative config，會把 global absolute root 與 read-only role 硬塞進不相符的資料模型，因此不採用。

### 以實際 path 作為 action identity 並重新驗證

`ProjectSkillInfo.path` 已提供唯一 absolute path。Agent Skills commands 改以 `skill_path` 定位文件、匯入、pull 與 delete；frontend API wrappers、row keys 與 action loading keys 同步使用 `path`，不新增另一套 opaque identifier。

Backend 不直接信任 `skill_path`。每次 action 都重新執行 Agent Skills 專用掃描，僅在 exact path 命中 fresh result 時繼續，並使用該 server-side result 的 root role 與實際 path。項目已刪除、root 已離線、symlink alias 已改變或 client 傳入未掃描 path 時回傳既有 not-found error，不 fallback 到同名 primary／legacy item。

替代方案是傳 `root index + relative_path`，但 root ordering 會隨 override 與目錄存在狀態改變；傳 content hash 也不能唯一區分 identical copies。實際 path 已在 payload 中且能由 fresh scan 驗證，因此採用現有欄位。

### additional roots 一律 read-only

Agent Skills response 在既有 Skill fields 外加入 `read_only: bool`。primary root result 為 `false`，additional root result 為 `true`；global override 若指向 legacy 目錄，該目錄位於 primary precedence，故為 `false`。

read-only result 可讀取文件，也可匯入中央 Library。匯入只複製或更新中央項目，不註冊 global target、不部署 primary copy，且操作前後 legacy source 必須保持不變。pull、delete 與從該 row 移除 managed target 均不提供 UI action；backend 對直接 IPC 呼叫仍回傳 invalid-input error。primary result 繼續走既有 import、sync target、pull 與 delete 邏輯。

替代方案是把 pull 解讀成「寫入 primary」或匯入後自動部署 primary，但這會讓使用者在 legacy row 上觸發與按鈕語意不同的寫入，且同名 primary conflict 可能被覆蓋，因此不採用。

### UI 顯示來源並以 path 區分狀態

Frontend 增加 Agent Skills 專用 result type，包含既有 `ProjectSkill` fields 與 `read_only`。列表 card、detail sheet、action keys、loading state 與選取 identity 使用 `skill.path`；detail 顯示實際 path，read-only row 顯示本地化 badge，保留文件檢視與 upload，隱藏 pull、delete、remove-managed actions。

英文、簡體中文與繁體中文各新增 read-only 來源文案。既有 project detail 使用的 `ProjectSkill` contract 不變，避免把 Agent Skills 專用狀態擴散到 project workspace。

替代方案是由 frontend 比較 `~/.agents/skills` 與 `~/.codex/skills` 字串推導 root role；這無法處理 home path、global override、symlink alias 與 custom adapter metadata，因此 root role 必須由 backend 決定。

## Implementation Contract

- **Observable behavior:** Codex 只有 `~/.codex/skills/legacy-tool` 時，Agent Skills 畫面會列出該 Skill、顯示 read-only 狀態並能開啟文件或匯入中心；legacy 來源不出現 pull、delete 或 remove-managed action。modern 與 legacy identical copy 只顯示 primary；different-content 同名 copy 以各自 path 顯示兩筆。
- **Interface / data shape:** `get_global_local_skills` 回傳 Agent Skills 專用 DTO，保留現有 Skill fields 並新增 `read_only: boolean`。`get_global_local_skill_document`、`import_global_local_skill_to_center`、`update_global_local_skill_from_center` 與 `delete_global_local_skill` 的 item identity 改為 `skill_path`；對應 TypeScript wrappers 傳送 `skill.path`。
- **Write contract:** read-only import 只能修改中央 Library，不得改寫 source、建立 agent target 或部署 primary copy。read-only pull/delete 回傳 `invalid_input`；未掃描或消失的 path 回傳 `not_found`。primary actions 維持既有行為。
- **Failure modes:** additional root 不存在或無法讀取時沿用掃描器安全行為跳過；action 前 root 消失時 fail closed，不用 relative path 或同名結果 fallback；無法 canonicalize 的 root 不建立、不搬移、不刪除資料。
- **Acceptance criteria:** Rust tests 覆蓋 legacy-only、primary-only、alias root、identical/different-content copies、override-as-legacy、path collision、arbitrary path rejection、read-only document/import/pull/delete 與 primary regression。`npm run build` 驗證 TypeScript contract 與三語文案引用；manual GUI check 驗證兩筆同名 rows、path badge 與 read-only actions。完整 `cargo test --manifest-path src-tauri/Cargo.toml`、`spectra validate`、`spectra analyze --json` 與 `git diff --check` 必須通過。
- **In scope:** `agent_workspace` global local scan/actions、Agent Skills IPC type、WorkspaceView identity/actions、三個現有 locale 檔與 regression tests。
- **Out of scope:** project workspace、路徑 migration、settings/DB schema、sync engine policy、file watcher、其他 artifact 類型與衝突解決流程。

## Risks / Trade-offs

- [Risk] absolute path 由 client 回傳可能被偽造 → backend 只接受 fresh server-side scan 中 exact path 命中的項目，並使用 scan result 而不是 client metadata 執行。
- [Risk] legacy source 在列表與 action 之間被外部刪除或替換 → action 重新掃描並 fail closed，不嘗試同名 fallback。
- [Risk] identical copies 去重後 legacy location 不在 UI 顯示 → primary precedence 符合 writable target 語意；只有內容衝突才需要兩筆可見來源。
- [Risk] read-only import 與 primary import 的 target 註冊不同 → 以 root role 明確分支並用測試斷言 legacy source 與 target table 均不變。
- [Risk] additional roots 增加 workspace scan I/O → canonical root 先去重，維持 flat scan，file watcher 已負責觸發 refresh，不新增 polling。

## Migration Plan

不需要資料或設定 migration。部署後下一次 Agent Skills refresh 直接顯示 discovery-only roots；rollback 還原 commands、TypeScript wrapper、WorkspaceView 與文案即可，磁碟與資料庫沒有需要回復的轉換。

## Open Questions

無；legacy 來源採 read-only、import 不自動部署、實際 path 作為經 fresh scan 驗證的 identity，均已在本 change 定案。
