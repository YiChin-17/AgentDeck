# AgentDeck product identity verification

Date: 2026-08-12

## Automated acceptance

| Check | Exit | Result |
| --- | ---: | --- |
| `node --test scripts/check-legacy-compatibility.test.mjs scripts/product-identity-metadata.test.mjs scripts/product-identity-display.test.mjs scripts/product-identity-icon.test.mjs scripts/check-product-identity.test.mjs` | 0 | 76 passed, 0 failed |
| `npm run build` | 0 | TypeScript and Vite production build completed |
| `npm run lint` | 0 | ESLint completed without findings |
| `npm run check:i18n` | 0 | `en` and `zh-TW` locale integrity passed |
| `npm run check:product-identity` | 0 | All checked product and legacy-allowlist boundaries passed |
| `npm run cli:build` | 0 | Explicit `skills-manager-cli` binary compiled |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 0 | 514 passed, 0 failed; desktop, CLI and doc-test targets also completed with 0 failures |
| `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --no-deps --format-version 1` | 0 | Package `agentdeck`; targets `app_lib`, `agentdeck`, and `skills-manager-cli` resolved |

The debug macOS bundle was built with `npm run tauri -- build --debug --bundles app --no-sign`. Its resolved `Info.plist` contains:

- `CFBundleDisplayName`: `AgentDeck`
- `CFBundleName`: `AgentDeck`
- `CFBundleExecutable`: `agentdeck`
- `CFBundleIdentifier`: `io.github.yichin17.agentdeck`
- `CFBundleIconFile`: `icon.icns` (631,288 bytes)

## Desktop icon (task 3.2)

The Dock icon of the running `AgentDeck.app` was captured and matches the AgentDeck
master: a white squircle holding four stacked blue-to-purple cards, still readable at
Dock size.

The in-app sidebar logo was a second, independent copy of the 32 px output at
`public/icons/32x32.png` and had been left on the upstream artwork; regenerating the
Tauri icons does not touch it. It now carries the AgentDeck 32 px output
(`bfb98922c4843ed409f91c3499168fae609b8499cc66317f08206894193e5158`, identical to
`src-tauri/icons/32x32.png`) and `scripts/check-product-identity.mjs` gained an
`in-app-logo` rule that fails when the two files diverge.

## 16 px app icon (task 3.4)

Downscaling the master to 16 px collapsed the four cards into one dark block, so a
dedicated simplified artwork now backs that size only: fewer cards, wider gaps, thicker
white borders. `scripts/build_small_icon.py` draws it into
`src-tauri/icons/icon-source-small.png` and replaces exactly three `icon.icns` slots —
`is32`, `s8mk` (16×16 @1x) and `ic11` (16×16 @2x). It needs `pip install icnsutil`
alongside Pillow; like Pillow, that is documented in the script docstring rather than in a
dependency manifest, since the repository has none.

Everything else was verified to be byte-identical against a copy of `icon.icns` taken
before the edit (`2a1398fb44499c8bda2afd8ba5f2d3b9856323579ff09a7751fb7bed05ea4a7e`): the
six Dock/Launchpad slots and the three 32 px slots `il32`/`l8mk`/`ic12` all hash the same.
`src-tauri/icons/icon-source.png`, `src-tauri/icons/icon.png`, `src-tauri/icons/32x32.png`
and `public/icons/32x32.png` are unchanged, so the approved Dock icon and the sidebar logo
are untouched. Re-running the script in a clean virtualenv reproduces the same
`icon.icns` byte for byte, and re-running `scripts/build_macos_icon.py` after its
shared-function refactor still emits the identical `icon.png`.

Decoding the result with `iconutil -c iconset` and inspecting the 16 px output shows a
dark front card with three stepped blue-to-purple bands separated by white gaps — the deck
now reads as a stack at that size. One consequence worth knowing: macOS picks a slot by
point size, not by pixel count, so a 16 pt icon on a Retina display draws `ic11` (the
simplified artwork at 32 physical px) while a 32 pt icon draws `ic12` (the original
four-card artwork at the same physical size). That is the intended trade of keeping the
change to 16 px only.

## Tray icon (task 3.3)

`tray-icon-16/20/24/32.png` are monochrome with transparency, `src-tauri/src/lib.rs`
selects the monochrome source on macOS and the colour source elsewhere, and
`icon_as_template(true)` is applied on macOS. The 852-byte `tray-icon-32.png` is
embedded verbatim in the built binary.

The menu bar was screenshotted in both Light and Dark appearance with the item revealed.
The four-card deck silhouette renders as expected and stays legible in both; the original
Light setting was restored immediately afterwards. The tray menu itself reads
`AgentDeck`, `Open AgentDeck`, `Check for skill updates`, `Open Skills Folder`, `Quit`.

Two things about this machine are worth recording for anyone repeating the check. Ice, a
menu-bar manager, keeps the status item in its hidden section — the accessibility API
reported position `{-419, 20}` until the item was revealed, and the glyph sitting in the
menu bar before that belongs to another app, since it stays there after AgentDeck quits.
And the desktop wallpaper keeps the menu bar dark under both appearances, so a genuinely
light menu bar background was never exercised on screen. Rendering the shipped assets the
way template mode paints them covers that case: black on a light bar, white on a dark bar,
legible at 20/24/32 px. At 16 px the card separations close up and the shape reads as one
solid block, which does not affect macOS — it loads the 32 px source.

## Runtime checks — three Library configurations (task 5.2)

The old App was closed before each launch. `~/Library/Application Support/skills-manager/repo-config.json`
was backed up first and restored byte-identically afterwards (`diff` clean).

| Configuration | `library_base` | Result |
| --- | --- | --- |
| Internal | (none) | Online. Dock, window, App menu (`About AgentDeck`, `Hide AgentDeck`, `Quit AgentDeck`), sidebar label, Settings footer `AgentDeck 1.30.0`, and central repository path `~/.skills-manager` all correct; no offline or migration banner. |
| External online | temporary directory carrying a matching `.agentdeck-library.json` | Online. Settings showed the configured external path, `目前使用自訂中央儲存路徑`, and `技能庫可使用 — 可使用`. No `skills` directory was created inside the external root, matching `startup_dirs_to_create`. |
| External offline | `/Volumes/AgentDeckTestVolume/library` (absent) | Offline guard fired: `技能庫離線 … 找不到該資料夾，磁碟可能沒有接上`, with add/update/delete/sync/backup blocked. The path was not created, and neither `/Volumes/AgentDeckTestVolume` nor its child exists after the run. |

Data was unchanged across all three launches. `~/.skills-manager` held the same 9 files
before and after (listing hash `ef7b7090af70418b9abe32f7261db1bc9fde1047482f6cf928055437d44f67d9`),
and the SQLite counts stayed at 0 skills, 1 scenario, 1 project. The only content change
was the app writing its own `backup_device_name` settings row during normal operation.

No Git backup is configured on this machine and the Keychain holds no
`skills-manager-git-backup` entry, so the credential check could only confirm that the
Backup page still offers the legacy default repository name `skills-manager-backup`. An
existing credential surviving the Bundle ID switch remains covered only by the
temporary-directory Rust tests.

The old App bundle and all user data remain untouched; nothing was deleted automatically.

## Still unverified at runtime

Everything below is covered by tests or source inspection but was never exercised against
real state on this machine.

- **localStorage does not survive the Bundle ID change.** WebKit keys its data directory
  by bundle identifier: `~/Library/WebKit/com.agentskills.desktop` and
  `~/Library/WebKit/skills-manager` both hold `skills-manager.projectAddCalloutDismissed`,
  and `~/Library/WebKit/io.github.yichin17.agentdeck` does not. Keys that the app rewrites
  from SQLite settings (`language`, `theme`, `skills-manager.viewedPresetId`) reappear on
  first launch, so the visible loss is limited to UI-local flags — a dismissed callout
  comes back once. The "Existing local preference is read in the same container" scenario
  holds only under its own hedge; a real Bundle ID change is not the same container.
- **Legacy Keychain credential.** No Git backup is configured here and the Keychain holds
  no `skills-manager-git-backup` entry, so an existing credential surviving the switch is
  covered only by the temporary-directory Rust tests.
- **Existing backup repository.** No real backup with `.skills-manager` metadata,
  `Skills-Manager-*` trailers, or `refs/skills-manager/*` was opened; only the protocol
  tests cover it.
- **External Library configured by the previous bundle.** The offline guard was verified
  against a path configured under the new bundle. The code path is the same, but the
  literal precondition in the scenario was not reproduced.
- **Windows and Linux.** `.ico` and the Square assets pass existence and dimension checks
  and the colour tray variant is selected by `cfg(not(target_os = "macos"))`, but neither
  was ever rendered on those platforms.
- **Release build.** All runtime evidence comes from a `--debug --no-sign` bundle. The
  signed release bundle, its icon resources, and the Gatekeeper path are untested.

The CLI contract was exercised for real: `skills-manager-cli --json --skills-root <tmp>`
returned well-formed JSON for `repo status`, `skills list`, and `presets list`, with
`base_dir`/`db_path` still under `.skills-manager` and `skills-manager.db`. The temporary
root it created under `~/.skills-manager/external/` was removed afterwards.

## Unrelated observation

`get_projects` took 73,609 ms on one launch (19 ms on another) for the single project on
an external volume, leaving the sidebar's project section empty until it returned. The
project row was never lost. This predates the rename and is outside this change.
