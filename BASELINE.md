# AgentDeck 上游基準

本文件保存 Phase 0 在目前 checkout 重新執行的上游來源、建置、測試與 production dependency audit 證據。所有路徑均相對於專案根目錄，結果不得由過往研究或本機 Git 設定推定。

## 驗證資訊

- 執行日期：2026-08-09
- 驗證 checkout：`786b383423d3ceab816abfcbaa1538f803bb1957`
- 上游基準 commit：`ab2a6947062c49640b751d4c2a9d8be816347dc1`
- 上游基準 tag：`v1.30.0`
- License：保留上游 `LICENSE` 內的 MIT License
- Git 版本：`2.55.0`

## 上游來源

- `origin`：`https://github.com/YiChin-17/AgentDeck.git`
- `upstream`：`https://github.com/xingkongliang/skills-manager.git`

### 來源驗證證據

| 執行日期 | 命令 | Exit status | 結果摘要 |
| --- | --- | ---: | --- |
| 2026-08-09 | `git --version` | 0 | `git version 2.55.0` |
| 2026-08-09 | `git rev-parse HEAD` | 0 | 驗證 checkout 為 `786b383423d3ceab816abfcbaa1538f803bb1957`。 |
| 2026-08-09 | `git remote -v` | 0 | `origin` 的 fetch／push URL 均為 AgentDeck repository；`upstream` 的 fetch／push URL 均為 Skills Manager repository。 |
| 2026-08-09 | `git rev-parse upstream/main` | 0 | 回傳完整 commit `ab2a6947062c49640b751d4c2a9d8be816347dc1`。 |
| 2026-08-09 | `git describe --tags --exact-match ab2a694` | 0 | 回傳 exact tag `v1.30.0`。 |
| 2026-08-09 | 人工比對 `LICENSE` | N/A | 首行為 `MIT License`，原 copyright 與完整 MIT 授權條款仍在。 |

## 前端基準

工具版本：Node.js `v26.4.0`；npm `11.17.0`。

| 執行日期 | 命令 | Exit status | 結果摘要 |
| --- | --- | ---: | --- |
| 2026-08-09 | `node --version` | 0 | `v26.4.0` |
| 2026-08-09 | `npm --version` | 0 | `11.17.0` |
| 2026-08-09 | `npm ci` | 0 | 依 `package-lock.json` 安裝 371 個 packages；npm 同時摘要 10 個 vulnerabilities，production 範圍由下方獨立 audit 判定。 |
| 2026-08-09 | `npm run build` | 0 | `tsc -b && vite build` 完成；Vite 轉換 2,110 個 modules，production assets 成功輸出至忽略追蹤的 `dist/`。另有既存的 chunk size 與 Browserslist 資料版本警告，未造成 build 失敗。 |
| 2026-08-09 | `git diff -- package.json package-lock.json` | 0 | 無輸出；鎖定安裝與 build 未改寫 frontend manifest 或 lockfile。 |

## Rust 基準

工具版本：rustc `1.97.0 (2d8144b78 2026-07-07)`；cargo `1.97.0 (c980f4866 2026-06-30)`。

| 執行日期 | 命令 | Exit status | 結果摘要 |
| --- | --- | ---: | --- |
| 2026-08-09 | `rustc --version` | 0 | `rustc 1.97.0 (2d8144b78 2026-07-07)` |
| 2026-08-09 | `cargo --version` | 0 | `cargo 1.97.0 (c980f4866 2026-06-30)` |
| 2026-08-09 | `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | workspace test 完成；合計 402 passed、0 failed、0 ignored。主要 library suite 為 402 passed，另有三個 0-test targets；compile 約 1 分 28 秒，主要 suite 約 11.13 秒。 |
| 2026-08-09 | `git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock` | 0 | 無輸出；tests 未改寫 Rust manifest 或 lockfile。 |

## Production dependency audits

audit 工具版本：npm `11.17.0`；cargo-audit `0.22.2`。兩項 audit 均在 manifests／lockfiles 尚未修改時執行。

| 執行日期 | 命令 | Exit status | Production advisory 結果 |
| --- | --- | ---: | --- |
| 2026-08-09 | `npm audit --omit=dev` | 1 | 2 個 high severity findings；受影響套件為 `react-router` 及相依的 `react-router-dom`。npm 表示可由 `npm audit fix` 修復。 |
| 2026-08-09 | `cargo audit --file src-tauri/Cargo.lock` | 1 | 掃描 728 個 crate dependencies，發現 11 個 vulnerabilities；受影響套件為 `quick-xml`、`rkyv`、`rustls-webpki`、`tar`。另報告 26 個 allowed warnings，未計入 vulnerability 數。 |

### 修復前 advisory 明細

- npm：`react-router`／`react-router-dom` 共 2 個 high findings；報告列出 12 個相關 GitHub advisory IDs，鎖定版本分別為 `react-router 7.13.1`、`react-router-dom 7.13.1`。
- Rust `quick-xml 0.38.4` 與 `0.39.4`：`RUSTSEC-2026-0195`、`RUSTSEC-2026-0194`，共 4 個 findings，修復版本為 `>=0.41.0`。
- Rust `rkyv 0.7.46`：`RUSTSEC-2026-0235`，修復版本為 `>=0.8.17`。
- Rust `rustls-webpki 0.103.9`：`RUSTSEC-2026-0104`、`RUSTSEC-2026-0098`、`RUSTSEC-2026-0099`、`RUSTSEC-2026-0049`，共 4 個 findings；同一 `0.103.x` line 的最高要求為 `>=0.103.13`。
- Rust `tar 0.4.44`：`RUSTSEC-2026-0068`、`RUSTSEC-2026-0067`，共 2 個 findings，修復版本為 `>=0.4.45`。

### 相容修復結果

只修改 `package-lock.json` 與 `src-tauri/Cargo.lock`；`package.json`、`src-tauri/Cargo.toml` 及應用程式碼無差異。

| 執行日期 | 命令 | Exit status | 結果摘要 |
| --- | --- | ---: | --- |
| 2026-08-09 | `npm audit fix --omit=dev` | 0 | 在既有 `react-router-dom ^7.13.1` range 內將 `react-router` 與 `react-router-dom` 由 `7.13.1` 更新至 `7.18.2`；production audit 當次回報 0 vulnerabilities。npm 同步產生的非安全 root package version 差異已排除。 |
| 2026-08-09 | `npm ci` | 0 | 修復後依 lockfile 重新安裝 371 個 packages；完整 dependency graph 摘要仍有 8 個 development-inclusive vulnerabilities（2 low、6 high），不屬於本 change 的 production audit 範圍。 |
| 2026-08-09 | `npm run build` | 0 | 修復後 production build 完成，Vite 轉換 2,110 個 modules 並成功輸出 assets；既存 chunk size 與 Browserslist 警告未造成失敗。 |
| 2026-08-09 | `npm audit --omit=dev` | 0 | 0 production vulnerabilities。原本 2 個 high findings 已排除。 |
| 2026-08-09 | `cargo update --manifest-path src-tauri/Cargo.toml -p rustls-webpki@0.103.9 --precise 0.103.13` | 0 | lockfile-only patch update；排除 `rustls-webpki` 的 4 個 findings。 |
| 2026-08-09 | `cargo update --manifest-path src-tauri/Cargo.toml -p tar@0.4.44 --precise 0.4.45` | 0 | lockfile-only patch update；排除 `tar` 的 2 個 findings。 |
| 2026-08-09 | `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 修復後合計 402 passed、0 failed、0 ignored；主要 suite 約 10.95 秒。 |
| 2026-08-09 | `cargo audit --file src-tauri/Cargo.lock` | 1 | vulnerabilities 由 11 降至 5；`rustls-webpki` 與 `tar` advisories 已排除。仍有 26 個 allowed warnings。 |

### 後續 remediation 前的未解 production advisories

- `quick-xml 0.38.4` 與 `0.39.4` 仍有 `RUSTSEC-2026-0195`、`RUSTSEC-2026-0194`，共 4 個 findings；修復要求 `quick-xml >=0.41.0`，跨越目前 `0.x` minor 相容界線。它們分別由 Tauri／`plist` 與 Linux Wayland dependency path 帶入。
- `rkyv 0.7.46` 仍有 `RUSTSEC-2026-0235`，共 1 個 finding；修復要求 `rkyv >=0.8.17`，跨越目前 `0.x` minor 相容界線，並由 logging path 的 `byte-unit`／`rust_decimal` 帶入 lockfile。
- 以上是首次建立基準時尚未解決的 5 個 findings；下方 `resolve-rust-production-advisories` remediation 已排除它們。這段保留作為修復前證據，不代表目前 audit 狀態。

## Rust production advisory remediation

執行日期：2026-08-09。工具版本：rustc `1.97.0 (2d8144b78 2026-07-07)`；cargo `1.97.0 (c980f4866 2026-06-30)`；cargo-audit `0.22.2`。

### 修復前 dependency paths

| 命令 | Exit status | 結果摘要 |
| --- | ---: | --- |
| `cargo tree --locked --manifest-path src-tauri/Cargo.toml --target all -i quick-xml@0.38.4` | 0 | `quick-xml 0.38.4` 由 `plist 1.8.0` 經 Tauri、tauri-codegen 與 tauri-plugin paths 帶入。 |
| `cargo tree --locked --manifest-path src-tauri/Cargo.toml --target all -i quick-xml@0.39.4` | 0 | `quick-xml 0.39.4` 由 `wayland-scanner 0.31.10` 經 Linux Wayland clipboard path 帶入。 |
| `cargo tree --locked --manifest-path src-tauri/Cargo.toml --target all -e all -i rkyv@0.7.46` | 0 | 無 active feature path；`rkyv 0.7.46` 是 `rust_decimal 1.40.0` 的 optional lockfile entry。 |
| `cargo tree --locked --manifest-path src-tauri/Cargo.toml --target all -e features -i rust_decimal` | 0 | owning path 為 `tauri-plugin-log 2.8.0` → `byte-unit 5.2.0` → `rust_decimal 1.40.0`。 |

### 最小 parent dependency 候選

- `plist 1.10.0` 正式依賴 `quick-xml ^0.41.0`；既有 consumers 接受 `plist ^1`。此版本宣告 MSRV 為 Rust 1.88，因此最終 manifest 必須如實反映新的 build toolchain 下限。
- `wayland-scanner 0.31.11` 正式依賴 `quick-xml ^0.41`；既有 Wayland consumers 接受 `wayland-scanner ^0.31.10`。
- `tauri-plugin-log 2.9.0` 移除 `byte-unit` dependency，使 unused `rust_decimal`／`rkyv 0.7` subtree 從 lockfile 消失；既有 `tauri-plugin-log = "2"` constraint 接受此版本。
- 三個候選維持 Tauri 2 與原有 platform features。實際 resolver 與最終 lockfile diff 由後續 remediation 指令驗證，不以 metadata 推論取代執行結果。

### 實際 remediation 結果

| 命令 | Exit status | 結果摘要 |
| --- | ---: | --- |
| `cargo update --manifest-path src-tauri/Cargo.toml -p plist --precise 1.10.0` | 0 | `plist 1.8.0` → `1.10.0`；其 owning path 改用 `quick-xml 0.41.0`。 |
| `cargo update --manifest-path src-tauri/Cargo.toml -p wayland-scanner --precise 0.31.11` | 0 | `wayland-scanner 0.31.10` → `0.31.11`；移除第二個 `quick-xml 0.39.4`，兩條 paths 共用 `quick-xml 0.41.0`。 |
| `cargo update --manifest-path src-tauri/Cargo.toml -p tauri-plugin-log --precise 2.9.0` | 0 | `tauri-plugin-log 2.8.0` → `2.9.0`；移除不再被使用的 `byte-unit`、`rust_decimal`、`rkyv 0.7.46` 及其專屬 subtree。 |
| `cargo tree --manifest-path src-tauri/Cargo.toml --target all` | 0 | macOS、Windows、Linux dependency graph 完整解析；Tauri 2.10.2、Wayland scanner 0.31.11 與 tauri-plugin-log 2.9.0 paths 均保留。 |
| `cargo audit --file src-tauri/Cargo.lock` | 0 | 掃描 705 個 crate dependencies，0 vulnerabilities；三個目標 RustSec IDs 均未再出現。仍完整報告 26 個 allowed warnings。 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 修復後主要 library suite 為 402 passed、0 failed、0 ignored；另有三個 0-test targets。test profile 約 21.46 秒，主要 suite 約 10.13 秒。 |
| `npm ci` | 0 | 依鎖定檔安裝 371 個 packages；完整 dependency graph 仍摘要 8 個 development-inclusive vulnerabilities（2 low、6 high），不屬於本 Rust remediation 範圍。 |
| `npm run build` | 0 | `tsc -b && vite build` 成功；Vite 轉換 2,110 個 modules，約 1.65 秒完成 production assets。既存 Browserslist 與 chunk size 警告未造成失敗。 |

- `src-tauri/Cargo.toml` 的 `rust-version` 由 `1.77.2` 更新為 `1.88.0`，如實反映 `plist 1.10.0` 的 MSRV。CI workflows 使用 Rust `stable`，未固定較舊 toolchain。
- Cargo 1.97 在提高 manifest MSRV 後將 `src-tauri/Cargo.lock` 格式由 version 3 更新為 version 4。除三個 parent updates、`quick-xml` 合併、logging-only subtree 清除，以及先前已驗證的 `rustls-webpki`／`tar` patches 外，沒有其他 package version update。
- 沒有新增 cargo-audit ignore／allowlist，沒有修改 application source、platform feature 或 target dependency declaration。
- 本機為 macOS，因此沒有執行 Windows 或 Linux 的實際 compile checks；`cargo tree --manifest-path src-tauri/Cargo.toml --target all` 僅證明三個平台的完整 dependency graph 可解析，不把它記錄為跨平台編譯成功。
- 目前 Rust production audit 為 0 vulnerabilities；26 個 allowed warnings 仍由 cargo-audit 完整列出，未隱藏也未宣稱已修復。
