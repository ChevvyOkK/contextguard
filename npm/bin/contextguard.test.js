"use strict";

// Node's built-in test runner — no dependency, matching this project's
// low-dependency convention everywhere else (the plugin has zero, the CLI
// has few). Covers only the deterministic, network-free logic; the actual
// download-extract-exec path was verified by hand against the real v0.6.0
// GitHub release (all 3 platforms this machine could exercise directly:
// download, cache-hit skip, and real exit-code propagation from a real
// `contextguard budget` run) rather than mocked here.

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { TARGETS, resolveTarget, parseSha256Sidecar, sha256File } = require("./contextguard.js");

// The exact 5 combinations release.yml's build matrix produces — verified
// against a real release (`gh release view v0.6.0 --json assets`), not
// assumed from reading the workflow file alone.
const REAL_RELEASE_TARGETS = [
  "contextguard-aarch64-apple-darwin.tar.gz",
  "contextguard-aarch64-unknown-linux-gnu.tar.gz",
  "contextguard-x86_64-apple-darwin.tar.gz",
  "contextguard-x86_64-pc-windows-msvc.zip",
  "contextguard-x86_64-unknown-linux-gnu.tar.gz",
];

test("every TARGETS entry maps to an asset name that actually exists in a real release", () => {
  for (const { target, ext } of Object.values(TARGETS)) {
    const assetName = `contextguard-${target}.${ext}`;
    assert.ok(
      REAL_RELEASE_TARGETS.includes(assetName),
      `${assetName} is not a real release asset — TARGETS has drifted from release.yml's matrix`,
    );
  }
});

test("every real release asset is reachable from some platform/arch pair", () => {
  const reachable = new Set(Object.values(TARGETS).map(({ target, ext }) => `contextguard-${target}.${ext}`));
  for (const assetName of REAL_RELEASE_TARGETS) {
    assert.ok(reachable.has(assetName), `${assetName} exists in releases but no platform/arch resolves to it`);
  }
});

test("resolveTarget finds the right entry for each supported platform/arch", () => {
  assert.deepEqual(resolveTarget("darwin", "x64"), { target: "x86_64-apple-darwin", ext: "tar.gz" });
  assert.deepEqual(resolveTarget("darwin", "arm64"), { target: "aarch64-apple-darwin", ext: "tar.gz" });
  assert.deepEqual(resolveTarget("linux", "x64"), { target: "x86_64-unknown-linux-gnu", ext: "tar.gz" });
  assert.deepEqual(resolveTarget("linux", "arm64"), { target: "aarch64-unknown-linux-gnu", ext: "tar.gz" });
  assert.deepEqual(resolveTarget("win32", "x64"), { target: "x86_64-pc-windows-msvc", ext: "zip" });
});

test("resolveTarget returns undefined for an unsupported platform/arch instead of guessing", () => {
  assert.equal(resolveTarget("freebsd", "x64"), undefined);
  assert.equal(resolveTarget("win32", "arm64"), undefined);
  assert.equal(resolveTarget("linux", "ia32"), undefined);
});

test("parseSha256Sidecar reads the hash out of a real sha256sum-style line", () => {
  assert.equal(
    parseSha256Sidecar("a3c2...deadbeef  contextguard-x86_64-unknown-linux-gnu.tar.gz\n"),
    "a3c2...deadbeef",
  );
});

test("parseSha256Sidecar lowercases and trims, matching Get-FileHash's uppercase-hex output on Windows", () => {
  assert.equal(parseSha256Sidecar("  DEADBEEF  contextguard-x86_64-pc-windows-msvc.zip"), "deadbeef");
});

test("sha256File matches a hash computed independently for known content", () => {
  // "abc" -> the textbook SHA-256 test vector (FIPS 180-2, appendix B.1).
  const tmp = path.join(os.tmpdir(), `contextguard-sha256-test-${process.pid}`);
  fs.writeFileSync(tmp, "abc");
  try {
    assert.equal(sha256File(tmp), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  } finally {
    fs.rmSync(tmp, { force: true });
  }
});

test("package.json's os/cpu restriction fields agree with what TARGETS actually supports", () => {
  const pkg = require("../package.json");
  const platforms = new Set(Object.keys(TARGETS).map((k) => k.split("-")[0]));
  const arches = new Set(Object.keys(TARGETS).map((k) => k.split("-")[1]));
  assert.deepEqual(new Set(pkg.os), platforms);
  assert.deepEqual(new Set(pkg.cpu), arches);
});
