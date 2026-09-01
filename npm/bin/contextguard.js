#!/usr/bin/env node
"use strict";

// npx wrapper: downloads the prebuilt binary matching this package's own
// version from GitHub Releases (built by .github/workflows/release.yml —
// see that file for the exact target/archive-name matrix this mirrors),
// caches it locally, and execs it with the real CLI's own stdio and exit
// code. This file has no purpose beyond fetch-and-exec — every actual
// behavior lives in the Rust binary; keeping it that way means this
// wrapper never drifts out of sync with what `contextguard` itself does.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const https = require("https");

const PKG = require("../package.json");
const VERSION = PKG.version;

// Mirrors the release workflow's build matrix exactly — if that matrix
// changes, this must change with it, not before.
const TARGETS = {
  "darwin-x64": { target: "x86_64-apple-darwin", ext: "tar.gz" },
  "darwin-arm64": { target: "aarch64-apple-darwin", ext: "tar.gz" },
  "linux-x64": { target: "x86_64-unknown-linux-gnu", ext: "tar.gz" },
  "linux-arm64": { target: "aarch64-unknown-linux-gnu", ext: "tar.gz" },
  "win32-x64": { target: "x86_64-pc-windows-msvc", ext: "zip" },
};

function resolveTarget() {
  const key = `${process.platform}-${process.arch}`;
  const entry = TARGETS[key];
  if (!entry) {
    const supported = Object.keys(TARGETS).join(", ");
    fail(
      `No prebuilt contextguard binary for ${process.platform}/${process.arch}.\n` +
        `Supported: ${supported}.\n` +
        `Open an issue for this platform: https://github.com/ChevvyOkK/contextguard/issues`,
    );
  }
  return entry;
}

function fail(message) {
  process.stderr.write(`contextguard: ${message}\n`);
  process.exit(1);
}

function cacheDir() {
  return path.join(os.homedir(), ".contextguard", "bin", VERSION);
}

function binaryPath(dir) {
  return path.join(dir, process.platform === "win32" ? "contextguard.exe" : "contextguard");
}

// A plain https.get with manual redirect-following — GitHub Releases
// serves the actual asset from a signed S3 redirect, so a client that
// doesn't follow 302s silently downloads an HTML error page instead of
// the binary. No dependency for this: Node's own https module handles it
// fine as long as redirects are followed by hand.
function download(url, destPath, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "contextguard-npx-wrapper" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          if (redirectsLeft <= 0) {
            reject(new Error("too many redirects"));
            return;
          }
          res.resume();
          download(res.headers.location, destPath, redirectsLeft - 1).then(resolve, reject);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} fetching ${url}`));
          return;
        }
        const file = fs.createWriteStream(destPath);
        res.pipe(file);
        file.on("finish", () => file.close(resolve));
        file.on("error", reject);
      })
      .on("error", reject);
  });
}

function extract(archivePath, destDir) {
  // Windows' own System32\tar.exe (bsdtar, 1803+) auto-detects .zip too —
  // but a developer running this via Git Bash, WSL-adjacent tooling, or
  // any other environment that puts GNU tar ahead of it on PATH would
  // silently fail: GNU tar doesn't understand .zip at all. Rather than
  // assume which `tar` a given Windows user's PATH resolves to, branch on
  // the archive's own extension and use PowerShell's Expand-Archive for
  // .zip specifically — a cmdlet, not a PATH-dependent binary, so it can't
  // be shadowed the same way.
  const isZip = archivePath.toLowerCase().endsWith(".zip");
  const result = isZip
    ? spawnSync(
        "powershell.exe",
        ["-NoProfile", "-NonInteractive", "-Command", `Expand-Archive -LiteralPath '${archivePath}' -DestinationPath '${destDir}' -Force`],
        { stdio: "inherit" },
      )
    : spawnSync("tar", ["-xzf", archivePath, "-C", destDir], { stdio: "inherit" });

  if (result.error || result.status !== 0) {
    fail(
      `Could not extract the downloaded archive${result.error ? `: ${result.error.message}` : ` (exit ${result.status})`}. ` +
        `Try re-running, or grab the archive directly: https://github.com/ChevvyOkK/contextguard/releases`,
    );
  }
}

async function ensureBinary() {
  const dir = cacheDir();
  const bin = binaryPath(dir);
  if (fs.existsSync(bin)) {
    return bin;
  }

  const { target, ext } = resolveTarget();
  const assetName = `contextguard-${target}.${ext}`;
  const url = `https://github.com/ChevvyOkK/contextguard/releases/download/v${VERSION}/${assetName}`;

  fs.mkdirSync(dir, { recursive: true });
  const archivePath = path.join(dir, assetName);

  process.stderr.write(`contextguard: downloading v${VERSION} for ${target}...\n`);
  try {
    await download(url, archivePath);
  } catch (err) {
    fail(
      `Could not download ${url}: ${err.message}\n` +
        `Check your connection, or grab the archive directly: https://github.com/ChevvyOkK/contextguard/releases`,
    );
  }

  extract(archivePath, dir);
  fs.rmSync(archivePath, { force: true });

  if (process.platform !== "win32") {
    fs.chmodSync(bin, 0o755);
  }
  if (!fs.existsSync(bin)) {
    fail(`Downloaded and extracted ${assetName}, but ${path.basename(bin)} wasn't in it. This is a packaging bug, not a network problem — please report it.`);
  }

  return bin;
}

async function main() {
  const bin = await ensureBinary();
  const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    fail(`Failed to run ${bin}: ${result.error.message}`);
  }
  process.exit(result.status === null ? 1 : result.status);
}

// Only run as a side effect when invoked directly (npx, or `node
// bin/contextguard.js`) — required() by the test file, this exports the
// deterministic, network-free pieces instead.
if (require.main === module) {
  main();
} else {
  module.exports = { TARGETS, resolveTarget: resolveTargetForTest };
}

// resolveTarget() above calls fail() -> process.exit() on a miss, which
// would kill the test runner itself. This is the same lookup without that
// side effect, so a test can assert on "returns undefined" instead of
// "exits the process".
function resolveTargetForTest(platform, arch) {
  return TARGETS[`${platform}-${arch}`];
}
