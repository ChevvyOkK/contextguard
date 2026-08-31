<div align="center">

# ContextGuard

**Local-first Runtime Guard & Efficiency Layer for Claude Code**  
*Detects no-progress loops, preserves important context across `/compact`, and reduces avoidable Claude Code context waste.*

[![CI](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml/badge.svg)](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-0.6.0-6366f1)](Cargo.toml)
[![Rust MSRV](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT OR Apache--2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![i18n](https://img.shields.io/badge/i18n-EN%20%7C%20RU-informational)](src/i18n.rs)
[![GitHub Stars](https://img.shields.io/github/stars/ChevvyOkK/contextguard?style=social)](https://github.com/ChevvyOkK/contextguard/stargazers)

[Quick Start](#-quick-start) · [How It Works](#-how-it-works) · [Detectors](#-the-six-detectors) · [Lossless Vault](#-smart-output-guard--lossless-vault) · [CLAUDE.md Lint](#-claudemd-diagnostics) · [Web Dashboard](https://contextguard-web.vercel.app)

</div>

<br>

> [!IMPORTANT]
> **100% Local-First by Construction.** ContextGuard runs alongside Claude Code on your own machine.
> - **CLI Analytics**: Reads session transcripts from `~/.claude/projects/` strictly offline.
> - **Runtime Plugin**: Inspects local hook events to halt loops and restore forgotten constraints.
> - **Zero Code Egress**: Source code, raw prompts, and conversations **never leave your machine**. Zero network calls unless you explicitly pass `--push`.

<br>

## 🛡️ How It Works: The 4-Stage Runtime Model

ContextGuard operates as an active safety and continuity layer hooked directly into the Claude Code lifecycle:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 1. OBSERVE (Claude Code Lifecycle Hooks)                                │
│    • Tool calls, bash execution outputs, test runner outcomes           │
│    • /compact trigger events and local token/cost burn patterns        │
├─────────────────────────────────────────────────────────────────────────┤
│ 2. DETECT (Zero-Lag Local Heuristics)                                  │
│    • Semantic No-Progress: same root test failure survived 3+ edits     │
│    • Continuity Risk: critical CLAUDE.md constraints need re-injection  │
│    • Abnormal Burn Rate: local sessions burning faster than history     │
├─────────────────────────────────────────────────────────────────────────┤
│ 3. INTERVENE (Targeted Safeguards)                                      │
│    • Structured Force Rethink protocol injected directly into context   │
│    • Opt-in hard-stop for repeated identical calls                      │
│    • Automatic bash/grep output truncation with signal preservation     │
├─────────────────────────────────────────────────────────────────────────┤
│ 4. PRESERVE (Zero Data Loss)                                            │
│    • Lossless Vault: 100% of raw output archived locally to disk        │
│    • Continuity Guard: auto-restores critical architectural rules       │
└─────────────────────────────────────────────────────────────────────────┘
```

<br>

## 📺 Live Demonstration

```text
$ contextguard --days 7

ContextGuard — Claude Code Session & Quota Audit

Sessions analyzed: 16
Tokens — input: 1,180,302 | cache-write: 839,516 | cache-read: 14,812,004 | output: 401,552
Estimated cost: $21.92 | Cache reuse efficiency: 88%

Observed Runtime Inefficiencies:
  [!] 3 No-Progress loops detected (same test failure across multiple edits)
  [!] 18 Repeated unchanged file reads (wasting ~34k cache tokens)
  [!] 2 Context amnesia events after /compact (critical rules dropped)
  [!] CLAUDE.md is 412 lines (~3,900 tokens) — re-read on every turn

Cost & Quota Optimization Engine:

  Saved ≈ $4.18
  Session 4f9a2b7c: Cache invalidated on turns 2+ (61% cache-write penalty).
  Fix: Keep stable content (rules, schemas) at prompt start, changing code at end.

  Saved ≈ $1.32
  src/handlers/payment.rs was re-read 5 times in session 2c8e0d15 without edits.
  Fix: Cache reference or Grep specific functions instead of re-reading full file.

  Saved ≈ $9.60
  CLAUDE.md is 412 lines and gets re-sent every turn.
  Fix: Run `contextguard lint --fix` to prune ~200 lines of boilerplate.
```

> [!TIP]
> Every diagnostic provides three clear lines: **Impact, Root Cause, and Fix.** No complex dashboards required — you get the exact terminal diff to run.

<br>

## ⚡ Quick Start

```bash
# 1. Install ContextGuard CLI (prebuilt binary for macOS, Linux, Windows)
curl -sSf https://contextguard-web.vercel.app/install.sh | sh

# 2. Add the live Runtime Guard to Claude Code
/plugin marketplace add ChevvyOkK/contextguard-plugin

# 3. Run a local audit across your sessions
contextguard --days 7

# 4. Lint and auto-prune your CLAUDE.md
contextguard lint CLAUDE.md --fix

# 5. Output in Russian
contextguard --lang ru
```

<br>

## 🧠 Core Detectors

Six deterministic analyzers in [`src/optimize.rs`](src/optimize.rs) inspect usage patterns and protect your workflow:

| Detector | What it catches | How it intervenes |
|---|---|---|
| **No-Progress Breaker** | The same test failure survives multiple real edits | Injects **Structured Force Rethink** protocol |
| **Continuity Guard** | Important CLAUDE.md constraints should survive `/compact` | Captures and re-injects rule-like constraints |
| **Cache Churn Watcher** | Cache writes exceed cache reads on turns 2+ | Pinpoints cache-busting prompt shifts |
| **CLAUDE.md Amortizer** | `CLAUDE.md` exceeds recommended 200 lines | Identifies boilerplate lines and autofixes |
| **Re-Read Watcher** | Unchanged file read 3+ times in one session | Suggests targeted grepping or caching |
| **Burn-Rate Watcher** | Local session cost is an outlier versus enough prior sessions | Flags the anomalous session with the comparison used |

<br>

## 🗄️ Smart Output Guard & Lossless Vault

When commands produce massive output (e.g. `npm test`, `pytest -vv`, `cargo build`), ContextGuard:
1. **Truncates active context** to the head, tail, and key stacktrace/error lines (saving up to 85% token bloat).
2. **Archives 100% raw output** to `~/.claude/contextguard/vault/CG-XXXXX.log`.
3. **Tags the output** with a reference ID and writes a local evidence event with exact size impact.

```text
... [ContextGuard Lossless Vault: 450 lines archived locally as ref: CG-84A21 — full output preserved] ...
[ContextGuard: key error lines from omitted block]
  FAILED tests/test_payment.py::test_checkout - AssertionError: 400 != 200
```

To view or search the full raw log anytime:
```bash
# Recall full log or grep inside it
cat ~/.claude/contextguard/vault/CG-84A21.log
```

<br>

## 🩺 `contextguard lint` — CLAUDE.md Optimizer

`CLAUDE.md` is re-sent on **every single request** in Claude Code. Unnecessary instructions act as a permanent tax on your quota.

```bash
$ contextguard lint CLAUDE.md --fix

CLAUDE.md diagnostics — 412 lines, ~3,900 tokens

  [boilerplate]   L88   Always write clean code. -> restates model default
  [stale path]    L214  See `src/legacy-v1.rs`   -> no session touched this path
  [duplicate]     L302  Do not edit Cargo.lock   -> identical to L14

Done — 2 lines removed automatically with --fix (~18 tokens/turn).
```

<br>

## 🤖 CI/CD Integration

Check PR changes to `CLAUDE.md` automatically in GitHub Actions:

```yaml
# .github/workflows/claude-md-lint.yml
name: CLAUDE.md Cost & Bloat Check
on: pull_request

permissions:
  pull-requests: write

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: ChevvyOkK/contextguard@v0.6.0
```

<br>

## ⚙️ CLI Command Reference

| Command | Purpose |
|---|---|
| `contextguard` | Full local audit across all Claude Code sessions |
| `contextguard --days 7` | Audit the last 7 days of activity |
| `contextguard lint [PATH] [--fix]` | Lint and prune `CLAUDE.md` bloat |
| `contextguard context` | Breakdown of tokens inside the active 200k window |
| `contextguard savings` | Report of tokens and quota saved by the plugin |
| `contextguard evidence [--limit 20]` | List recent local guard evidence events |
| `contextguard recall <event-or-vault-id>` | Print a full vaulted output or evidence event by ID |
| `contextguard budget --max <USD>` | Exit 1 if local spending exceeds budget limit |
| `contextguard git-cost` | Attribute session token costs to git branches and PRs |

<br>

## 🔒 Privacy Architecture

- **Zero telemetry by default**: No analytics, no tracking, no phone-home.
- **Zero code egress**: Prompts and source files remain strictly on your local disk.
- **Optional team sync**: Only activated if you explicitly run `contextguard --push`, sending daily aggregate numbers (token counts and session volume only, never code or conversation text).

<br>

## 📄 License

Licensed under either of [MIT license](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE) at your option.
