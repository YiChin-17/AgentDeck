## Why

Phase 0 的 production dependency audit 在 2026-08-09 仍發現 `quick-xml` 的 4 個 vulnerabilities 與 `rkyv` 的 1 個 vulnerability。修復版本跨越現有 `0.x` 相容界線並由 Tauri、Wayland 與 logging 相依鏈帶入，不能在上游基準 change 內以 lockfile patch 安全處理。

## What Changes

- 追查 `quick-xml 0.38.4`、`quick-xml 0.39.4` 與 `rkyv 0.7.46` 的完整 production dependency paths，選擇支援修復版本的最小上游依賴組合。
- 更新 `src-tauri/Cargo.toml` 與 `src-tauri/Cargo.lock` 內必要的 Rust dependencies，消除 `RUSTSEC-2026-0194`、`RUSTSEC-2026-0195` 與 `RUSTSEC-2026-0235`。
- 驗證 macOS build／tests，並檢查 Windows、Linux target 的 dependency graph 仍可解析，避免以移除跨平台依賴規避 advisory。
- 重跑 Rust production audit 並保存修復前後證據，不以 ignore 設定或 allowlist 隱藏 findings。

## Non-Goals

- 不修改 AgentDeck 的使用者介面、資料模型、設定格式或執行行為。
- 不移除上游跨平台支援，也不為消除 lockfile finding 關閉既有 platform features。
- 不處理 cargo-audit 的 unmaintained、unsound 或 yanked allowed warnings；它們需要各自的風險與相依範圍評估。
- 不處理已由 `establish-upstream-baseline` 修復的 `rustls-webpki` 與 `tar` advisories。

## Capabilities

### New Capabilities

- `rust-production-advisory-remediation`: 規範跨相容界線的 Rust production dependency remediation、跨平台解析與 audit 驗證。

### Modified Capabilities

（無）

## Impact

- Plan phase: `plan.md` Phase 0 的後續安全 remediation。
- Affected specs: `rust-production-advisory-remediation`
- Affected code:
  - Modified: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
  - Conditionally modified only when a dependency API requires a mechanical compatibility update: `src-tauri/src/`
  - New: none
  - Removed: none
- Upstream compatibility: 保留 Tauri 2 架構及 macOS、Windows、Linux dependency targets；不刻意分歧上游執行行為。
