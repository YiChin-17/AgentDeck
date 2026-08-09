## Context

Codex adapter 目前以 `.codex/skills` 作為 global 與 project deployment primary，並只把 `.agents/skills` 當作 global additional discovery root。Phase 1 要反轉這個關係，而且 project scanner 也必須同時讀取新舊路徑。現有 `custom_tool_paths` 與 `custom_tool_project_paths` 已提供 global absolute override 與 project-relative override，不需要新增設定格式或 UI。

這項變更橫跨 adapter、global scanner、project scanner 與 project commands，因此需要一致的路徑優先順序與去重規則。它是 AgentDeck 對上游預設的刻意差異，但不移除 legacy 相容性。

驗證時確認 Agent Skills 畫面的 global local Skill 流程另有邊界：`agent_workspace` 的列表與後續 actions 只以 primary root 加 `relative_path` 定位。若直接把 legacy root 加入列表，modern 與 legacy 的不同內容同名 Skill 會無法被文件讀取、匯入、更新與刪除 commands 唯一識別。該畫面與 IPC identity 契約不在本 change 內，改由 `support-legacy-codex-skills-in-agent-workspace` 處理。

## Goals / Non-Goals

**Goals:**

- Codex global 與 project deployment 預設使用 `.agents/skills`。
- `.codex/skills` 在 global 與 project scope 維持 discovery-only legacy root。
- 掃描同一實體目錄或相同內容時只產生一個可見 Skill，且 global discovery group 保留所有不同來源位置。
- global 與 project overrides 繼續決定部署主路徑，清除 override 後回到 `.agents/skills`。
- 其他 agent 的路徑與掃描行為不變。

**Non-Goals:**

- 不搬移或刪除 `.codex/skills` 內容。
- 不實作 Library offline 防護、Plugin path、Hook 或 Config Profile。
- 不改變 symlink／copy 策略、settings schema、IPC payload 或設定畫面。
- 不變更 Agent Skills 畫面的 global local Skill 列表、來源 identity 或 actions；global legacy Skill 在該畫面的呈現與操作延後至 `support-legacy-codex-skills-in-agent-workspace`。
- 不把內容不同但名稱相同的 Skill 合併；這類項目必須維持可見，讓既有衝突處理流程判斷。

## Decisions

### 將部署主路徑與 discovery-only 路徑分開

Codex adapter 的 global primary 與 project primary 都改為 `.agents/skills`，global additional discovery root 設為 `.codex/skills`。adapter 增加明確的 project additional discovery roots，使 project commands 不必用 agent key 寫死 Codex 特例。部署、enable／disable 與 install target 仍只取 primary 或 override；additional roots 永遠不可成為寫入目標。

替代方案是讓 project scanner 直接為 Codex 加上 `.codex/skills`。這會把產品規則散落到 scanner，且未來其他 adapter 需要 project fallback 時會重複條件判斷，因此不採用。

### 以 scan root precedence 與 canonical path 去重

每個 scope 的掃描順序固定為 explicit override（若有）或 modern primary，接著才是 legacy additional roots。開始遍歷前，以 canonical path 去除指向同一實體目錄的 roots；無法 canonicalize 的不存在或無權限路徑沿用既有安全行為並跳過，不建立、搬移或刪除資料。

替代方案是只比較路徑字串。這無法處理 symlink 或等價路徑，因此不足以避免重複掃描。

### 使用內容 identity 去重而不隱藏衝突

global discovery 沿用 name 加 content fingerprint 的 group identity，相同 identity 合成一個 group並保留不同 `found_path` locations。project discovery 對相同 agent、normalized name 與 content hash 的結果只保留 precedence 較高的項目；若 name 相同但 content hash 不同，兩者都保留，不把差異靜默覆蓋。

替代方案是只依 Skill 名稱去重。這會隱藏新舊路徑內容分歧，違反既有外部修改與衝突可見原則，因此不採用。

### 保留既有 override 儲存格式

global override 繼續使用 `custom_tool_paths["codex"]` 的 absolute path，project override 繼續使用 `custom_tool_project_paths["codex"]` 的 project-relative path。override 只取代 modern primary；legacy `.codex/skills` 仍為 discovery-only root。override 若指向 legacy root，canonical root 去重確保只掃描一次。reset 不需要資料 migration，會自然回復新的 `.agents/skills` default。

替代方案是新增 Codex 專用設定 key。既有資料模型已能表達所需行為，新增 key 只會增加 migration 與 UI 複雜度，因此不採用。

## Implementation Contract

- **Observable behavior:** 未設定 override 時，Codex global install target 為 `~/.agents/skills`，project install target 為 `<repo>/.agents/skills`；scanner-based global discovery 與 project workspace discovery 都能發現 legacy `.codex/skills`。相同內容不重複顯示，內容衝突不被合併。Agent Skills 畫面的 global local Skill 列表與 actions 不屬於此 observable contract。
- **Interface / data shape:** `ToolAdapter` 保留 `relative_skills_dir`、`project_relative_skills_dir`、`additional_scan_dirs` 與既有 override 欄位，並提供 project additional discovery roots。現有 Tauri commands、settings keys 與 frontend `ToolInfo` payload 不變。
- **Failure modes:** 不存在、無權限或無法 canonicalize 的 additional root 依既有 scanner 行為跳過；掃描不得建立或刪除目錄。override 驗證失敗仍回傳既有 input error，不新增 fallback。
- **Acceptance criteria:** Rust tests 必須覆蓋 Codex default paths、global/project legacy discovery、同一 canonical root、相同內容去重、不同內容保留、global/project override precedence 與 reset defaults。完整 `cargo test --manifest-path src-tauri/Cargo.toml`、`npm ci`、`npm run build` 必須 exit 0，且 `git diff` 不得包含其他 agent 路徑或 settings schema 變更。
- **In scope:** adapter path metadata、global/project discovery root resolution、去重與 regression tests。
- **Out of scope:** Library offline、資料搬移、UI redesign、新依賴、其他 artifact 類型、其他 agent default 變更，以及 Agent Skills 畫面的 global legacy Skill identity 與 actions。

## Risks / Trade-offs

- [Risk] `.agents/skills` 也可能被其他 agent 掃描，同一實體 Skill 會在不同 agent 分頁各自出現 → global 去重限定在各 adapter 的結果內，保留 agent assignment 語意，不跨 agent 強制合併。
- [Risk] legacy 與 modern 路徑同名但內容不同 → 兩筆都保留並暴露來源路徑，不以 precedence 靜默覆蓋衝突。
- [Risk] 提高 scanner 路徑數增加 I/O → 先做 canonical root 去重，且只掃描存在的 flat Skill roots，不新增 recursive walk。
- [Risk] 回滾預設會讓新部署再次寫入 `.codex/skills` → 回滾只需還原 adapter primary/additional metadata；不需要搬移使用者資料，已部署在 `.agents/skills` 的 Skills 仍可由原本 additional discovery 行為讀取。
