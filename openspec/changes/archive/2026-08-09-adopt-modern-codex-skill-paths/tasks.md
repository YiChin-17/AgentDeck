## 1. 建立 Codex 路徑契約

- [x] 1.1 依照「將部署主路徑與 discovery-only 路徑分開」實作「Modern Codex paths are the deployment defaults」與「Legacy Codex paths remain discovery-only sources」的 adapter metadata：global／project primary 為 `.agents/skills`，global／project legacy roots 為 `.codex/skills`，且 additional roots 不參與寫入；在 `src-tauri/src/core/tool_adapters.rs` 新增或更新單元測試，明確斷言 Codex 四種 path roles，並確認既有其他 adapter path tests 通過。
- [x] 1.2 依照「保留既有 override 儲存格式」完成「User overrides retain deployment precedence」：沿用 `custom_tool_paths["codex"]` 與 `custom_tool_project_paths["codex"]`，讓 override 只取代 deployment primary，reset 回復 `.agents/skills`，legacy roots 維持 discovery-only；以 `src-tauri/src/core/tool_adapters.rs` 與 `src-tauri/src/core/tool_service.rs` 的 focused tests 驗證 global／project override、override 等於 legacy root 及 reset 結果，不新增 settings key 或 IPC 欄位。

## 2. 掃描與去重

- [x] 2.1 依照「以 scan root precedence 與 canonical path 去重」讓 global discovery 先掃描 override 或 modern primary，再掃描存在的 legacy root，且同一 canonical root 只遍歷一次；在 `src-tauri/src/core/scanner.rs` 以 temporary directories 與 symlink test 驗證 modern-only、legacy-only、兩個路徑與 alias root，並確認掃描不建立、搬移或刪除 Skill 目錄。
- [x] 2.2 讓 project discovery 同時讀取 primary 與 legacy roots，而部署、enable／disable 及刪除目標仍只使用 primary；在 `src-tauri/src/core/project_scanner.rs` 與 `src-tauri/src/commands/projects.rs` 以 temporary project tests 驗證 `.agents/skills`、`.codex/skills`、project override 與 disabled root 的解析結果。
- [x] 2.3 依照「使用內容 identity 去重而不隱藏衝突」完成「Equivalent Codex discovery results are deduplicated」：同 agent、normalized name 與 content hash 相同時只顯示一筆並依 precedence 選主結果，global group 保留所有不同 locations；同名但 hash 不同時保留兩筆。以 global grouping 與 project scanner tests 分別驗證 identical copies、conflicting copies 和 source path 結果。

## 3. 相容性與完整驗證

- [x] 3.1 完成「Other agent path behavior is unchanged」：執行 adapter、scanner、project scanner 與 project command 的 focused Rust tests，並以 `git diff` 確認 Claude Code、其他 built-in/custom adapters、settings schema、frontend IPC payload、symlink／copy 策略及 application UI 沒有行為差異。
- [x] 3.2 執行 `cargo test --manifest-path src-tauri/Cargo.toml`、`npm ci`、`npm run build`、`spectra validate adopt-modern-codex-skill-paths`、`spectra analyze adopt-modern-codex-skill-paths --json` 與 `git diff --check`；要求測試與 build 全部 exit 0、Spectra 沒有 Critical／Warning，並記錄 test counts、既有 warnings 與未執行的平台檢查，不把 Library offline 或其他 Phase 1 功能納入本 change。
