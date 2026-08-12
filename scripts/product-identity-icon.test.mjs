import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const masterPath = path.join(root, "src-tauri/icons/icon-source.png");
const upstreamMasterSha256 = "7a4602adb9dc8bedb51da6e9c6293007b5d577a24827e9d2ec37b4f5e0f50090";

function inspectPng(buffer) {
  assert.deepEqual([...buffer.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  assert.equal(buffer.toString("ascii", 12, 16), "IHDR");
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
    colorType: buffer[25],
    sha256: crypto.createHash("sha256").update(buffer).digest("hex"),
  };
}

function validateMaster(icon) {
  const errors = [];
  if (icon.width !== icon.height) errors.push("master must be square");
  if (icon.width < 1024) errors.push("master must be at least 1024px");
  if (icon.colorType !== 6) errors.push("master must be an RGBA PNG");
  if (icon.sha256 === upstreamMasterSha256) errors.push("master matches upstream artwork");
  return errors;
}

test("AgentDeck master icon is square, lossless RGBA and independent from upstream", () => {
  const icon = inspectPng(fs.readFileSync(masterPath));

  assert.deepEqual(validateMaster(icon), []);
});

test("PNG inspection rejects a master below 1024 pixels", () => {
  const fixture = Buffer.from(fs.readFileSync(masterPath));
  fixture.writeUInt32BE(512, 16);

  const icon = inspectPng(fixture);
  assert.deepEqual(validateMaster(icon), ["master must be square", "master must be at least 1024px"]);
});

test("desktop icon outputs exist with their required dimensions", () => {
  const expectedPngs = {
    "assets/icon.png": 128,
    "src-tauri/icons/icon.png": 1024,
    "src-tauri/icons/32x32.png": 32,
    "src-tauri/icons/64x64.png": 64,
    "src-tauri/icons/128x128.png": 128,
    "src-tauri/icons/128x128@2x.png": 256,
    "src-tauri/icons/StoreLogo.png": 50,
    "src-tauri/icons/Square30x30Logo.png": 30,
    "src-tauri/icons/Square44x44Logo.png": 44,
    "src-tauri/icons/Square71x71Logo.png": 71,
    "src-tauri/icons/Square89x89Logo.png": 89,
    "src-tauri/icons/Square107x107Logo.png": 107,
    "src-tauri/icons/Square142x142Logo.png": 142,
    "src-tauri/icons/Square150x150Logo.png": 150,
    "src-tauri/icons/Square284x284Logo.png": 284,
    "src-tauri/icons/Square310x310Logo.png": 310,
  };

  for (const [relativePath, expectedSize] of Object.entries(expectedPngs)) {
    const icon = inspectPng(fs.readFileSync(path.join(root, relativePath)));
    assert.equal(icon.width, expectedSize, relativePath);
    assert.equal(icon.height, expectedSize, relativePath);
  }

  for (const relativePath of ["src-tauri/icons/icon.icns", "src-tauri/icons/icon.ico"]) {
    assert.ok(fs.statSync(path.join(root, relativePath)).size > 0, `${relativePath} is empty`);
  }
});

test("macOS Tray uses a generated monochrome source in template mode", () => {
  const rust = fs.readFileSync(path.join(root, "src-tauri/src/lib.rs"), "utf8");
  const expectedPngs = {
    "src-tauri/icons/tray/tray-icon-source.png": 512,
    "src-tauri/icons/tray/tray-icon-16.png": 16,
    "src-tauri/icons/tray/tray-icon-20.png": 20,
    "src-tauri/icons/tray/tray-icon-24.png": 24,
    "src-tauri/icons/tray/tray-icon-32.png": 32,
  };

  assert.match(rust, /builder = builder\.icon_as_template\(true\)/);
  for (const [relativePath, expectedSize] of Object.entries(expectedPngs)) {
    const icon = inspectPng(fs.readFileSync(path.join(root, relativePath)));
    assert.equal(icon.width, expectedSize, relativePath);
    assert.equal(icon.height, expectedSize, relativePath);
  }
});
