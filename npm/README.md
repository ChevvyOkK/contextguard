# @chevvyokk/contextguard

`npx` wrapper for the [ContextGuard CLI](https://github.com/ChevvyOkK/contextguard) — the lowest-friction install path: no Rust toolchain, no `cargo install --git` wait.

> [!NOTE]
> **Not published to npm yet.** The code here is real, tested, and
> merged — `npm publish` is deliberately held until v1.0, alongside the
> CLI's move to a source-available license, so the first thing anyone
> ever sees on npm is the final license rather than an MIT/Apache release
> followed by a stricter one. Until then, `npx @chevvyokk/contextguard`
> below won't resolve — use `curl -sSf https://contextguard-web.vercel.app/install.sh | sh`
> instead (see the main README).

```bash
npx @chevvyokk/contextguard
```

Published under a scoped name because the plain `contextguard` (and even
`contextguard-cli`) names were already taken on npm by unrelated
packages — this avoids that confusion rather than fighting over a name
that isn't available.

## What this actually is

A ~150-line script, nothing more: on first run it downloads the prebuilt
binary matching your OS/CPU and this package's own version from
[GitHub Releases](https://github.com/ChevvyOkK/contextguard/releases),
caches it at `~/.contextguard/bin/<version>/`, and execs it with your
real arguments, stdio, and exit code. Every subsequent run is a cache hit
— no network, no re-download, just the real Rust binary running directly.

No behavior lives here. This wrapper has no logic of its own to drift out
of sync with `contextguard` itself — it only ever runs the real thing.

Supported today: macOS (Intel + Apple Silicon), Linux (x86_64 + ARM64),
Windows (x86_64) — the same five targets `release.yml` cross-compiles on
every tag. Anything else gets a clear "not supported, open an issue"
message instead of a confusing failure.

## Releasing a new version

This package's `version` must match a real, already-published CLI
release tag exactly (`npm version` here, then `npm publish` — after the
matching `vX.Y.Z` GitHub release exists, not before, or the first `npx`
run for that version 404s).
