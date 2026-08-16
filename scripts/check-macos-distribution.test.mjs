import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkMacosDistribution } from "./check-macos-distribution.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(root, "scripts", "check-macos-distribution.mjs");

const BUNDLE_ID = "io.github.yichin17.agentdeck";
const VERSION = "1.31.0";
const WORKFLOW = ".github/workflows/release.yml";
const DOC = "docs/macos-distribution.md";

/// The fixture workflow is written the way the committed workflow is written, so
/// the contract is asserted against a realistic release pipeline rather than
/// against a list of the checker's own tokens.
const CLEAN_WORKFLOW = `name: Build & Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

permissions:
  contents: read

env:
  EXPECTED_BUNDLE_ID: ${BUNDLE_ID}
  EPHEMERAL_KEYCHAIN: agentdeck-release.keychain-db
  EPHEMERAL_KEY_DIR: private_keys
  EPHEMERAL_CERTIFICATE: certificate.p12

jobs:
  contract:
    name: Distribution contract
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Check the committed distribution contract
        run: npm run check:macos-distribution

  regression:
    name: Phase 7 regressions
    if: startsWith(github.ref, 'refs/tags/')
    runs-on: macos-14
    permissions:
      contents: read
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - run: npm ci
      - name: Frontend production build
        run: npm run build
      - name: Lint
        run: npm run lint
      - name: Locale integrity
        run: npm run check:i18n
      - name: Repository Node contracts
        run: node --test scripts/*.test.mjs
      - name: Rust tests
        run: cargo test --locked --manifest-path src-tauri/Cargo.toml
      - name: Personal installation bundle
        run: npm run tauri:build
      - name: Personal installation contract
        run: npm run check:personal-installation

  build:
    name: Build macOS (\${{ matrix.arch }})
    if: startsWith(github.ref, 'refs/tags/')
    runs-on: macos-14
    environment: macos-release
    permissions:
      contents: read
    timeout-minutes: 60
    strategy:
      fail-fast: false
      matrix:
        include:
          - arch: arm64
            target: aarch64-apple-darwin
            dmg_arch: aarch64
          - arch: x86_64
            target: x86_64-apple-darwin
            dmg_arch: x64
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Verify tag, committed versions, and branch history
        run: |
          set -euo pipefail
          case "\$GITHUB_REF_NAME" in
            v[0-9]*.[0-9]*.[0-9]*) ;;
            *) echo "Refusing a ref that is not a release tag" >&2; exit 1 ;;
          esac
          TAG_VERSION="\${GITHUB_REF_NAME#v}"
          PKG_VERSION="\$(node -p "require('./package.json').version")"
          TAURI_VERSION="\$(node -p "require('./src-tauri/tauri.conf.json').version")"
          [ "\$TAG_VERSION" = "\$PKG_VERSION" ] || exit 1
          [ "\$TAG_VERSION" = "\$TAURI_VERSION" ] || exit 1
          git fetch --no-tags origin main
          git merge-base --is-ancestor "\$GITHUB_SHA" origin/main
          echo "VERSION=\$TAG_VERSION" >> "\$GITHUB_ENV"
      - name: Import Developer ID credentials into an ephemeral keychain
        env:
          APPLE_CERTIFICATE: \${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: \${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_API_ISSUER: \${{ secrets.APPLE_API_ISSUER }}
          APPLE_API_KEY: \${{ secrets.APPLE_API_KEY }}
          APPLE_API_KEY_BASE64: \${{ secrets.APPLE_API_KEY_BASE64 }}
          APPLE_TEAM_ID: \${{ vars.APPLE_TEAM_ID }}
        run: |
          set -euo pipefail
          umask 077
          KEYCHAIN_PATH="\$RUNNER_TEMP/\$EPHEMERAL_KEYCHAIN"
          CERT_PATH="\$RUNNER_TEMP/\$EPHEMERAL_CERTIFICATE"
          security create-keychain -p "\$KEYCHAIN_PASSWORD" "\$KEYCHAIN_PATH"
          security import "\$CERT_PATH" -k "\$KEYCHAIN_PATH" -P "\$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
          security find-identity -v -p codesigning "\$KEYCHAIN_PATH"
          echo "APPLE_SIGNING_IDENTITY=\$IDENTITY" >> "\$GITHUB_ENV"
      - name: Build signed and notarized bundles
        run: npm run tauri:build -- --target \${{ matrix.target }} --bundles app,dmg
      - name: Verify Developer ID signing, notarization, and Gatekeeper
        run: |
          set -euo pipefail
          APP_PATH="src-tauri/target/\${{ matrix.target }}/release/bundle/macos/AgentDeck.app"
          DMG_PATH="src-tauri/target/\${{ matrix.target }}/release/bundle/dmg/AgentDeck_\${VERSION}_\${{ matrix.dmg_arch }}.dmg"
          BUNDLE_ID="\$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "\$APP_PATH/Contents/Info.plist")"
          APP_VERSION="\$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "\$APP_PATH/Contents/Info.plist")"
          [ "\$BUNDLE_ID" = "\$EXPECTED_BUNDLE_ID" ] || exit 1
          [ "\$APP_VERSION" = "\$VERSION" ] || exit 1
          codesign --verify --deep --strict --verbose=2 "\$APP_PATH"
          SIG_INFO="\$(codesign -dvvv "\$APP_PATH" 2>&1)"
          grep -q 'Authority=Developer ID Application:' <<<"\$SIG_INFO"
          grep -q "TeamIdentifier=\$APPLE_TEAM_ID" <<<"\$SIG_INFO"
          grep -q 'Timestamp=' <<<"\$SIG_INFO"
          grep -qE 'flags=[^)]*runtime' <<<"\$SIG_INFO"
          xcrun stapler validate "\$APP_PATH"
          spctl --assess --type execute --verbose=4 "\$APP_PATH"
          xcrun stapler validate "\$DMG_PATH"
          hdiutil attach "\$DMG_PATH" -readonly -nobrowse -mountpoint "\$MOUNT_POINT"
          MOUNTED_APP="\$(find "\$MOUNT_POINT" -maxdepth 1 -name '*.app' | head -n1)"
          [ "\$(basename "\$MOUNTED_APP")" = "AgentDeck.app" ] || exit 1
          hdiutil detach "\$MOUNT_POINT" -quiet
      - name: Generate the SHA-256 checksum
        run: |
          set -euo pipefail
          DMG_NAME="AgentDeck_\${VERSION}_\${{ matrix.dmg_arch }}.dmg"
          shasum -a 256 "\$DMG_NAME" > "\$DMG_NAME.sha256"
          shasum -a 256 -c "\$DMG_NAME.sha256"
      - name: Upload the verified artifacts
        uses: actions/upload-artifact@v4
        with:
          name: agentdeck-macos-\${{ matrix.arch }}
          path: |
            dist/release/AgentDeck_*.dmg
            dist/release/AgentDeck_*.dmg.sha256
          if-no-files-found: error
      - name: Remove the ephemeral credentials
        if: always()
        run: |
          security delete-keychain "\$RUNNER_TEMP/\$EPHEMERAL_KEYCHAIN" || true
          rm -f "\$RUNNER_TEMP/\$EPHEMERAL_CERTIFICATE"
          rm -rf "\$RUNNER_TEMP/\$EPHEMERAL_KEY_DIR"

  publish:
    name: Publish release
    needs: [contract, regression, build]
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')
    runs-on: ubuntu-latest
    permissions:
      contents: write
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - name: Download the verified artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist/incoming
      - name: Verify the complete artifact set
        run: |
          set -euo pipefail
          VERSION="\${GITHUB_REF_NAME#v}"
          for name in "AgentDeck_\${VERSION}_aarch64.dmg" "AgentDeck_\${VERSION}_x64.dmg"; do
            shasum -a 256 -c "\$name.sha256"
          done
          [ "\$ASSET_COUNT" -eq 4 ] || exit 1
      - name: Stage the release draft
        run: |
          set -euo pipefail
          if gh release view "\$GITHUB_REF_NAME" --repo "\$REPO" >/dev/null 2>&1; then
            echo "Refusing to overwrite an existing release" >&2
            exit 1
          fi
          gh release create "\$GITHUB_REF_NAME" --repo "\$REPO" --target "\$GITHUB_SHA" --title "AgentDeck \$GITHUB_REF_NAME" --notes-file "\$BODY_FILE" --draft dist/release/*
      - name: Verify the staged draft
        run: |
          set -euo pipefail
          gh release view "\$GITHUB_REF_NAME" --repo "\$REPO" --json isDraft,tagName,targetCommitish,assets > draft.json
          shasum -a 256 -c "\$name.sha256"
      - name: Publish the verified draft
        run: gh release edit "\$GITHUB_REF_NAME" --repo "\$REPO" --draft=false
`;

/// Written as the guide a downloader would follow if AgentDeck were ever
/// distributed. While it is personal-only there is nothing to download, so the
/// guide keeps every verification step but states each one as a condition.
const CLEAN_DOC = `# macOS distribution (not currently active)

There is currently no public AgentDeck release: no GitHub Release exists and
nothing is offered for download. This guide is dormant material describing how a
release would be verified if distribution were ever authorized.

## Which download would apply

- Apple silicon (M-series): \`AgentDeck_${VERSION}_aarch64.dmg\`
- Intel (x64): \`AgentDeck_${VERSION}_x64.dmg\`

## How the checksum would be verified

\`\`\`bash
shasum -a 256 ~/Downloads/AgentDeck_${VERSION}_aarch64.dmg
\`\`\`

You would compare the digest with the matching \`.sha256\` asset from the same
release. That file holds the digest and the disk image basename.

## What signature, notarization, and Gatekeeper would mean

Each application inside a disk image would carry a Developer ID Application
certificate, would be notarized by Apple and stapled, so Gatekeeper would accept
it on first launch without any workaround.

## No application auto-update

AgentDeck has no application auto-update. A newer version would be installed by
downloading the next disk image by hand.

## Withdrawal

If a release ever had to be withdrawn, the maintainer would turn it back into a
draft and keep the tag, so the affected assets stop being downloadable.
`;

const CLEAN_README = `<h1 align="center">AgentDeck</h1>

## Personal installation (local build)

This personal build has no application auto-update, no public release hosting,
no Developer ID signing and no notarization guarantee.

## macOS distribution (not currently active)

There is currently no public AgentDeck release. See
[docs/macos-distribution.md](docs/macos-distribution.md) for how a signed disk
image would be verified if distribution were ever authorized.
`;

function defaultFixture() {
  return {
    "package.json": JSON.stringify(
      {
        name: "agentdeck",
        version: VERSION,
        scripts: { "check:macos-distribution": "node scripts/check-macos-distribution.mjs" },
      },
      null,
      2,
    ),
    "src-tauri/tauri.conf.json": JSON.stringify(
      { productName: "AgentDeck", version: VERSION, identifier: BUNDLE_ID },
      null,
      2,
    ),
    [WORKFLOW]: CLEAN_WORKFLOW,
    [DOC]: CLEAN_DOC,
    "README.md": CLEAN_README,
  };
}

function withFixture(mutate, callback) {
  const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "agentdeck-macos-distribution-"));
  try {
    const files = defaultFixture();
    mutate?.(files);
    for (const [relativePath, contents] of Object.entries(files)) {
      if (contents === null) continue;
      const filePath = path.join(fixtureRoot, relativePath);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, contents, "utf8");
    }
    return callback(fixtureRoot);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

function codesFor(mutate) {
  return withFixture(mutate, (fixtureRoot) => {
    const { failures } = checkMacosDistribution({ rootDir: fixtureRoot });
    return [...new Set(failures.map((failure) => failure.code))].sort();
  });
}

/// Documentation rules overlap — removing a paragraph can drop a required topic
/// and a required statement at once — so these assert the rule that fired, not
/// only the finding code.
function findingsFor(mutate) {
  return withFixture(mutate, (fixtureRoot) => {
    const { failures } = checkMacosDistribution({ rootDir: fixtureRoot });
    return {
      codes: [...new Set(failures.map((failure) => failure.code))].sort(),
      rules: failures.map((failure) => failure.message).join(" | "),
    };
  });
}

function runFixture(mutate) {
  return withFixture(mutate, (fixtureRoot) =>
    spawnSync(process.execPath, [checkerPath, "--root", fixtureRoot], { encoding: "utf8" }),
  );
}

function editWorkflow(files, replace, replacement = "") {
  const next = files[WORKFLOW].replace(replace, replacement);
  assert.notEqual(next, files[WORKFLOW], `fixture mutation did not change the workflow: ${replace}`);
  files[WORKFLOW] = next;
}

test("clean distribution tree reports the stable success summary", () => {
  const result = runFixture();

  assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
  assert.equal(
    result.stdout.trim(),
    "macOS distribution contract passed: product=AgentDeck targets=arm64,x86_64 updater=absent publish=staged",
  );
});

test("legacy product name in the release title reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, '--title "AgentDeck $GITHUB_REF_NAME"', '--title "Skills Manager $GITHUB_REF_NAME"');
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("legacy bundle path in the verification gate reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "macos/AgentDeck.app", "macos/skills-manager.app");
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("wrong committed bundle identifier reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    files["src-tauri/tauri.conf.json"] = JSON.stringify(
      { productName: "AgentDeck", version: VERSION, identifier: "com.example.agentdeck" },
      null,
      2,
    );
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("committed versions differing between package and bundle report tag_version_mismatch", () => {
  const codes = codesFor((files) => {
    files["package.json"] = JSON.stringify(
      {
        name: "agentdeck",
        version: "1.30.0",
        scripts: { "check:macos-distribution": "node scripts/check-macos-distribution.mjs" },
      },
      null,
      2,
    );
  });

  assert.deepEqual(codes, ["tag_version_mismatch"]);
});

test("release workflow without a tag-to-version gate reports tag_version_mismatch", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, 'TAG_VERSION="${GITHUB_REF_NAME#v}"', 'TAG_VERSION="1.31.0"');
  });

  assert.deepEqual(codes, ["tag_version_mismatch"]);
});

test("release workflow without a protected-history gate reports tag_version_mismatch", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, '          git merge-base --is-ancestor "$GITHUB_SHA" origin/main\n');
  });

  assert.deepEqual(codes, ["tag_version_mismatch"]);
});

test("workflow-level write permission reports release_authority_too_broad", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "permissions:\n  contents: read", "permissions:\n  contents: write");
  });

  assert.deepEqual(codes, ["release_authority_too_broad"]);
});

test("build job holding write permission reports release_authority_too_broad", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "    environment: macos-release\n    permissions:\n      contents: read",
      "    environment: macos-release\n    permissions:\n      contents: write",
    );
  });

  assert.deepEqual(codes, ["release_authority_too_broad"]);
});

test("publish job without write permission reports release_authority_too_broad", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "    permissions:\n      contents: write\n    timeout-minutes: 20",
      "    permissions:\n      contents: read\n    timeout-minutes: 20",
    );
  });

  assert.deepEqual(codes, ["release_authority_too_broad"]);
});

test("build job outside the protected environment reports release_environment_missing", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "    environment: macos-release\n");
  });

  assert.deepEqual(codes, ["release_environment_missing"]);
});

test("Apple secrets used outside the protected build job report secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "      - name: Publish the verified draft\n",
      "      - name: Publish the verified draft\n        env:\n          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}\n",
    );
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("printing a credential value reports secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "          security find-identity -v -p codesigning \"$KEYCHAIN_PATH\"",
      "          echo \"$APPLE_CERTIFICATE\"",
    );
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("uploading the private key directory reports secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "            dist/release/AgentDeck_*.dmg.sha256",
      "            dist/release/AgentDeck_*.dmg.sha256\n            ${{ runner.temp }}/private_keys",
    );
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("credential cleanup that can be skipped reports secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "      - name: Remove the ephemeral credentials\n        if: always()\n",
      "      - name: Remove the ephemeral credentials\n",
    );
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("cleanup that can only find the keychain through an exported value reports secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    // The export happens after the import and identity checks, so a failure
    // there leaves the cleanup step with an empty path and the keychain alive.
    editWorkflow(
      files,
      '          echo "APPLE_SIGNING_IDENTITY=$IDENTITY" >> "$GITHUB_ENV"',
      '          echo "APPLE_SIGNING_IDENTITY=$IDENTITY" >> "$GITHUB_ENV"\n          echo "KEYCHAIN_PATH=$KEYCHAIN_PATH" >> "$GITHUB_ENV"',
    );
    editWorkflow(
      files,
      '          security delete-keychain "$RUNNER_TEMP/$EPHEMERAL_KEYCHAIN" || true',
      '          security delete-keychain "$KEYCHAIN_PATH" || true',
    );
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("legacy product name in a disk image filename reports identity_mismatch", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      '          DMG_NAME="AgentDeck_${VERSION}_${{ matrix.dmg_arch }}.dmg"',
      '          DMG_NAME="Skills_Manager_1.31.0_aarch64.dmg"',
    );
  });

  assert.deepEqual(codes, ["identity_mismatch"]);
});

test("publish job that does not wait for the regression gate reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "    needs: [contract, regression, build]", "    needs: [contract, build]");
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

/// Each Phase 7 gate the change promises to re-run before publication. Dropping
/// any one of them has to fail, not just dropping the job.
const REQUIRED_REGRESSION_GATES = [
  { label: "the frontend production build", line: "        run: npm run build" },
  { label: "lint", line: "        run: npm run lint" },
  { label: "the locale integrity check", line: "        run: npm run check:i18n" },
  { label: "the repository Node contracts", line: "        run: node --test scripts/*.test.mjs" },
  {
    label: "the Rust tests",
    line: "        run: cargo test --locked --manifest-path src-tauri/Cargo.toml",
  },
  {
    label: "the personal installation contract",
    line: "        run: npm run check:personal-installation",
  },
];

for (const { label, line } of REQUIRED_REGRESSION_GATES) {
  test(`regression job without ${label} reports publish_order_invalid`, () => {
    const codes = codesFor((files) => {
      editWorkflow(files, `${line}\n`, "        run: true\n");
    });

    assert.deepEqual(codes, ["publish_order_invalid"]);
  });
}

test("removing the regression job entirely reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, /  regression:\n[\s\S]*?\n\n  build:/, "  build:");
    editWorkflow(files, "    needs: [contract, regression, build]", "    needs: [contract, build]");
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

test("reintroducing the updater signing key reports updater_asset_present", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "        run: npm run tauri:build -- --target ${{ matrix.target }} --bundles app,dmg",
      "        env:\n          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n        run: npm run tauri:build -- --target ${{ matrix.target }} --bundles app,dmg,updater",
    );
  });

  assert.deepEqual(codes, ["updater_asset_present"]);
});

test("publishing an update manifest reports updater_asset_present", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "            dist/release/AgentDeck_*.dmg.sha256",
      "            dist/release/AgentDeck_*.dmg.sha256\n            dist/release/latest.json",
    );
  });

  assert.deepEqual(codes, ["updater_asset_present"]);
});

test("missing Gatekeeper assessment reports verification_gate_missing", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, '          spctl --assess --type execute --verbose=4 "$APP_PATH"\n');
  });

  assert.deepEqual(codes, ["verification_gate_missing"]);
});

test("missing TeamIdentifier assertion reports verification_gate_missing", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, '          grep -q "TeamIdentifier=$APPLE_TEAM_ID" <<<"$SIG_INFO"\n');
  });

  assert.deepEqual(codes, ["verification_gate_missing"]);
});

test("skipping the mounted disk image reports verification_gate_missing", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      '          hdiutil attach "$DMG_PATH" -readonly -nobrowse -mountpoint "$MOUNT_POINT"\n',
    );
  });

  assert.deepEqual(codes, ["verification_gate_missing"]);
});

test("missing checksum generation reports checksum_missing", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      '          shasum -a 256 "$DMG_NAME" > "$DMG_NAME.sha256"\n          shasum -a 256 -c "$DMG_NAME.sha256"\n',
      "          true\n",
    );
  });

  assert.deepEqual(codes, ["checksum_missing"]);
});

test("publish job that does not wait for the builds reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "    needs: [contract, regression, build]", "    needs: [contract, regression]");
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

test("publish job reachable from workflow_dispatch reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')\n    runs-on: ubuntu-latest",
      "    runs-on: ubuntu-latest",
    );
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

test("creating a public release without staging reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "--notes-file \"$BODY_FILE\" --draft dist/release/*", "--notes-file \"$BODY_FILE\" dist/release/*");
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

test("overwriting existing release assets reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(files, "--notes-file \"$BODY_FILE\" --draft", "--notes-file \"$BODY_FILE\" --draft --clobber");
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

test("build matrix covering a single architecture reports publish_order_invalid", () => {
  const codes = codesFor((files) => {
    editWorkflow(
      files,
      "          - arch: x86_64\n            target: x86_64-apple-darwin\n            dmg_arch: x64\n",
    );
  });

  assert.deepEqual(codes, ["publish_order_invalid"]);
});

/// The publication job is the only place where "both architectures arrived" can
/// still be decided, so each way the asset set can come up short is checked.
const INCOMPLETE_ASSET_SETS = [
  {
    label: "the Apple silicon disk image is not required",
    from: '"AgentDeck_${VERSION}_aarch64.dmg" ',
    to: "",
  },
  {
    label: "the Intel disk image is not required",
    from: ' "AgentDeck_${VERSION}_x64.dmg"',
    to: "",
  },
  {
    label: "an incomplete asset count is accepted",
    from: '[ "$ASSET_COUNT" -eq 4 ]',
    to: '[ "$ASSET_COUNT" -eq 2 ]',
  },
];

for (const { label, from, to } of INCOMPLETE_ASSET_SETS) {
  test(`publish job where ${label} reports publish_order_invalid`, () => {
    const codes = codesFor((files) => {
      editWorkflow(files, from, to);
    });

    assert.deepEqual(codes, ["publish_order_invalid"]);
  });
}

test("a generated checksum line carries the digest and the disk image basename", () => {
  const digestLine = withFixture(null, (fixtureRoot) => {
    const name = `AgentDeck_${VERSION}_aarch64.dmg`;
    fs.writeFileSync(path.join(fixtureRoot, name), "fixture disk image", "utf8");
    const result = spawnSync("shasum", ["-a", "256", name], {
      cwd: fixtureRoot,
      encoding: "utf8",
    });
    assert.equal(result.status, 0, `${result.stdout}${result.stderr}`);
    return result.stdout.replace(/\n$/, "");
  });

  assert.match(digestLine, new RegExp(`^[0-9a-f]{64} {2}AgentDeck_${VERSION}_aarch64\\.dmg$`));
});

test("missing official distribution guide reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files[DOC] = null;
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("guide without checksum verification reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files[DOC] = CLEAN_DOC.replace(
      /## How the checksum would be verified[\s\S]*?## What signature/,
      "## What signature",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("guide claiming an active public release reports documentation_incomplete", () => {
  const { codes, rules } = findingsFor((files) => {
    files[DOC] = CLEAN_DOC.replace(
      "## Which download would apply",
      "Tagged versions are published as a GitHub Release.\n\n## Which download would apply",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
  assert.match(rules, /active-release-claim/);
});

test("guide without the current-inactive statement reports documentation_incomplete", () => {
  const { codes, rules } = findingsFor((files) => {
    files[DOC] = CLEAN_DOC.replace(
      "There is currently no public AgentDeck release: no GitHub Release exists and\nnothing is offered for download.",
      "A GitHub Release would carry the disk images.",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
  assert.match(rules, /distribution-inactive/);
});

test("guide burying the current-inactive statement below the first section reports documentation_incomplete", () => {
  const { codes, rules } = findingsFor((files) => {
    files[DOC] = `${CLEAN_DOC.replace(
      "There is currently no public AgentDeck release: no GitHub Release exists and\nnothing is offered for download.",
      "A GitHub Release would carry the disk images.",
    )}
## Footnote

There is currently no public AgentDeck release.
`;
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
  assert.match(rules, /distribution-inactive/);
});

test("conditional future-release instructions stay allowed", () => {
  const codes = codesFor((files) => {
    files[DOC] = `${CLEAN_DOC}
## If distribution is ever authorized

When a version is published, each disk image would arrive with its own
\`.sha256\` file, and the official AgentDeck download would then be the only
signed artifact.
`;
  });

  assert.deepEqual(codes, []);
});

test("README claiming an active public release reports documentation_incomplete", () => {
  const { codes, rules } = findingsFor((files) => {
    files["README.md"] = CLEAN_README.replace(
      "There is currently no public AgentDeck release.",
      "Signed and notarized disk images are published per version tag.",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
  assert.match(rules, /active-release-claim|distribution-inactive/);
});

test("guide instructing a Gatekeeper bypass reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files[DOC] = `${CLEAN_DOC}\nIf the disk image will not open, run xattr -cr AgentDeck.app.\n`;
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("guide recording a machine-specific path reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files[DOC] = CLEAN_DOC.replace("~/Downloads/", "/Users/maintainer/Downloads/");
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("README without a route to the official guide reports documentation_incomplete", () => {
  const codes = codesFor((files) => {
    files["README.md"] = CLEAN_README.replace(
      /\[docs\/macos-distribution\.md\]\(docs\/macos-distribution\.md\)/,
      "the distribution notes",
    );
  });

  assert.deepEqual(codes, ["documentation_incomplete"]);
});

test("guide carrying credential material reports secret_boundary_violation", () => {
  const codes = codesFor((files) => {
    files[DOC] = `${CLEAN_DOC}\n-----BEGIN PRIVATE KEY-----\nMIIBVgIBADANBg\n-----END PRIVATE KEY-----\n`;
  });

  assert.deepEqual(codes, ["secret_boundary_violation"]);
});

test("failing check exits non-zero and names the stable code and project-relative location", () => {
  const result = runFixture((files) => {
    editWorkflow(files, "macos/AgentDeck.app", "macos/skills-manager.app");
  });
  const output = `${result.stdout}${result.stderr}`;

  assert.notEqual(result.status, 0);
  assert.match(output, /identity_mismatch/);
  assert.match(output, /\.github\/workflows\/release\.yml/);
  assert.doesNotMatch(output, new RegExp(os.tmpdir().replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
});

test("checker reads committed files only, without network, subprocess, or secret access", () => {
  const source = fs.readFileSync(checkerPath, "utf8");

  assert.doesNotMatch(source, /node:https?|\bfetch\(|XMLHttpRequest|https?:\/\//);
  /// No subprocess at all. Without one the checker cannot run `security`,
  /// `codesign` or `notarytool`, so it can neither read the operator Keychain nor
  /// sign anything, whatever its rule table happens to name.
  assert.doesNotMatch(source, /node:child_process|execFileSync|spawnSync|execSync/);
  assert.doesNotMatch(source, /process\.env\b/);
  assert.doesNotMatch(source, /writeFileSync|mkdirSync|rmSync|unlinkSync|chmodSync/);
});

test("the committed repository satisfies the macOS distribution contract", () => {
  const { failures, summary } = checkMacosDistribution({ rootDir: root });

  assert.deepEqual(failures, []);
  assert.equal(
    summary,
    "macOS distribution contract passed: product=AgentDeck targets=arm64,x86_64 updater=absent publish=staged",
  );
});
