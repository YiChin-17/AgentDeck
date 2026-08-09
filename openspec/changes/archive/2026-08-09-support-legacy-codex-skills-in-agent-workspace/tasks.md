## 1. Global roots 掃描與去重

- [x] 1.1 依照「掃描 roots 並用 precedence 去重」在 `src-tauri/src/commands/agent_workspace.rs` 完成「Agent workspace discovers configured global roots」：Agent Skills 專用掃描依 primary、additional roots 順序產生含 absolute `path` 與 `read_only` root role 的 DTO，override 指向 legacy 時仍為 writable primary；以 focused Rust tests 驗證 legacy-only、primary-only、missing additional root、override-as-legacy 及 canonical alias 只遍歷一次。
- [x] 1.2 依照「掃描 roots 並用 precedence 去重」完成「Equivalent results are deduplicated without hiding conflicts」：相同 agent、normalized name、enabled state 與 content hash 只保留 precedence 較高結果，不同 hash 的同名結果保留各自 path；以 focused Rust tests 斷言 identical copy 回傳一筆 writable primary、conflicting copy 回傳兩筆不同 path，且非 Codex adapter 的 primary results 不變。

## 2. Verified identity 與 read-only actions

- [x] 2.1 依照「以實際 path 作為 action identity 並重新驗證」完成「Actions use verified source identity」：`get_global_local_skill_document`、`import_global_local_skill_to_center`、`update_global_local_skill_from_center` 與 `delete_global_local_skill` 以 `skill_path` 查找 fresh server-side scan result，不接受未掃描 path、不 fallback 到同名項目；以 Rust tests 驗證 modern／legacy 同 `relative_path` 仍讀到指定文件、arbitrary path 與 action 前消失的 path 回傳 not-found 且沒有 mutation。
- [x] 2.2 依照「additional roots 一律 read-only」完成「Discovery-only sources remain read-only」與「Primary source behavior remains unchanged」：legacy 文件可讀、import 只更新中央 Library 且不改 source／不建 target，legacy pull/delete 回傳 invalid-input；primary import、target registration、pull 與 delete 沿用既有流程。以 temporary roots、database target assertions、before/after content hash 與既有 `agent_workspace` regression tests 驗證兩種 root role。

## 3. Agent Skills UI 與 IPC

- [x] 3.1 依照「UI 顯示來源並以 path 區分狀態」更新 `src/lib/tauri.ts` 的 Agent Skills 專用 result type 與四個 action wrappers，讓 `read_only` 進入 frontend 且所有 item commands 傳送 `skill.path`；以 `npm run build` 驗證 TypeScript IPC contract，並以 source review 確認 project workspace 的 `ProjectSkill` API 沒有被改成 discovery-only contract。
- [x] 3.2 依照「UI 顯示來源並以 path 區分狀態」完成「Source state is visible in UI」：`src/views/WorkspaceView.tsx` 的 row key、action key、loading state 與 detail lookup 使用 absolute path，read-only rows 顯示實際 path 與本地化 badge，只保留 document 與 upload；在 `src/i18n/en.json`、`src/i18n/zh.json`、`src/i18n/zh-TW.json` 補齊文案，以 `npm run build` 驗證 TypeScript contract，並以 modern／legacy 不同內容的臨時同名 Skill 實機驗證兩張卡可分別開啟各自 path 與 document，legacy row 沒有 pull、delete、remove-managed actions。

## 4. 完整驗證

- [x] 4.1 執行 focused `agent_workspace` Rust tests、`cargo test --manifest-path src-tauri/Cargo.toml`、`npm run build`、`spectra validate support-legacy-codex-skills-in-agent-workspace`、`spectra analyze support-legacy-codex-skills-in-agent-workspace --json` 與 `git diff --check`；要求 tests/build exit 0、Spectra 無 Critical／Warning，並以 `git diff` 確認未修改 project workspace routing、settings/database schema、sync engine、file watcher、其他 artifact flows 或其他 agent primary behavior，未執行的平台檢查照實記錄。
