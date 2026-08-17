## Context

`.github/workflows/test.yml` 的 top-level `pull_request.paths` 只包含 Rust tree 與 workflow 本身，因此 frontend、locale、`package.json` 與 `scripts/` 的 PR 不會觸發一般 CI。Release regression job 已定義 Node validation commands，可作為 PR gate 的既有基準，但一般 PR 不應執行 bundle、signing 或 release 步驟。

## Goals / Non-Goals

**Goals:**

- 讓任何 pull request 都進入 test workflow，消除 path filter 遺漏新 surface 的風險。
- 新增獨立 Node validation job，以鎖定依賴執行 build、lint、i18n 與 repository Node contracts。
- 用 repository checker 鎖定 trigger 與 commands，並保留既有 Rust matrix 和 Linux check。

**Non-Goals:**

- 不修改 release／prepare-release workflow。
- 不在 PR job 建置安裝檔、執行 signing／notarization、使用 secrets 或發布 asset。
- 不新增 dependency 或第三方 action。
- 不移除或合併現有 Rust jobs。

## Decisions

### Pull requests 不使用 top-level path filter

移除 `pull_request.paths`，使 frontend、locale、workflow、package metadata 與目前或未來的 repository contract files 都能觸發 validation。相較維護一份持續擴張的 allowlist，無 filter 能避免新檔案類型再次繞過 CI；代價是純文件 PR 也會執行 checks，接受此成本以換取完整 gate。

### Node validation job 對齊 release regression commands

新增 Ubuntu Node 22 job，依序執行 `npm ci`、`npm run build`、`npm run lint`、`npm run check:i18n` 與 `node --test scripts/*.test.mjs`。這組命令與 release regression 的 frontend/repository 子集合一致；不把 Rust、Tauri bundle 或 personal-installation checks 複製進此 job。

### Repository checker 鎖定 PR workflow contract

新增零依賴 checker 解析 `.github/workflows/test.yml` 與 `package.json`，驗證 pull_request 不受 restrictive paths 限制、Node job 具必要 pinned setup action 與命令、既有 Rust job identifiers 仍存在。fixture tests 覆蓋缺少 trigger、command 與 Rust coverage 的獨立 finding，package script 提供可重複執行入口。

## Implementation Contract

- Behavior：任何 pull request 都會觸發 Test workflow；Node validation 任一 command 非零時該 job 失敗並阻擋對應 required check。既有 macOS／Windows Rust tests 與 Linux cargo check 繼續執行。
- Interface：新增 package script `check:pull-request-validation`；checker 成功 exit 0，違規時 exit non-zero 並輸出 finding rule 與 `.github/workflows/test.yml` 或 `package.json`。
- Failure modes：dependency install、build、lint、locale 或 Node contract test 任一失敗即停止 Node job；不得使用 `continue-on-error` 或條件式略過必要 commands。
- Acceptance：新 checker fixture tests、`npm run check:pull-request-validation`、`npm run build`、`npm run lint` 與 `npm run check:i18n` 全部通過，且 workflow source 明確保留三個 Rust platform checks。
- In scope：Test workflow 的 pull_request trigger、Node job、package script 與 checker fixtures。
- Out of scope：push trigger 策略、release workflows、bundle／distribution、branch protection 設定與 GitHub required-check 管理。

## Risks / Trade-offs

- [Risk] 所有 PR 都跑完整 Test workflow，增加 CI 時間 → Node 與 Rust jobs 平行，沿用 cache，且不在 PR 執行 bundle／release gates。
- [Risk] checker 與 workflow parser 對 YAML 排版敏感 → 沿用 repository 既有文字 contract checker 慣例，以多個小 fixture 固定允許的結構。
- [Risk] Node contract glob 納入新增測試後時間增加 → 與 release regression 使用相同 glob，避免兩條 validation 路徑漂移。
