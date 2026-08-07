<div align="center">

# ContextGuard

**Blazing-fast, local-first CLI in Rust that finds where your Claude Code tokens go —**
**cache churn, dead re-reads, and `CLAUDE.md` bloat, before it hits your invoice.**

[![CI](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml/badge.svg)](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-0.1.0-6366f1)](Cargo.toml)
[![Rust MSRV](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT OR Apache--2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![i18n](https://img.shields.io/badge/i18n-EN%20%7C%20RU-informational)](src/i18n.rs)
[![GitHub Stars](https://img.shields.io/github/stars/ChevvyOkK/contextguard?style=social)](https://github.com/ChevvyOkK/contextguard/stargazers)

[Quick Start](#-quick-start) · [Key Features](#-the-six-detectors) · [CI/CD](#-ci-integration-post-costs-on-every-pr) · [Web Dashboard](https://contextguard-web.vercel.app)

</div>

<br>

> [!IMPORTANT]
> **Local-first, by construction — not by promise.** ContextGuard parses the session
> transcripts Claude Code already writes to `~/.claude/projects/` **on your own disk**.
> It never reads or uploads your source code, prompts, conversation content, or API
> keys, and makes **zero network calls** unless you explicitly pass `--push`. Check
> the Network tab yourself — there's nothing to hide.

<br>

## 📺 See it in action

```text
$ contextguard --days 7

ContextGuard — Claude Code token usage audit

Sessions analyzed: 12
Tokens — input: 1 180 302 | cache-write: 839 516 | cache-read: 14 812 004 | output: 401 552
Estimated cost: $21.92
Cache efficiency: 88%

Most expensive sessions:
  $6.14  4f9a2b7c-91e0-4a3d-8f21-...
  $3.87  2c8e0d15-77aa-4b12-9c04-...
  $2.55  8b1f6a90-33dd-4e77-a105-...

Tools by call frequency:
    142  Read
     58  Edit
     31  Bash
     19  Grep

CLAUDE.md:
  Path: ./CLAUDE.md
  Length: 412 lines (~3921 tokens) — longer than the recommended 200

Suggestions:
  — CLAUDE.md is longer than the 200-line guideline and gets re-read every
    turn — trimming it saves real money at your session volume.

Cost-Optimization Engine:

  Lost ≈ $4.18
  Session 4f9a2b7c: the cache is being rewritten instead of reused — 61%
  of tokens on turns 2+ paid the cache-write price (14 turns).
  Fix: Keep stable content (files, CLAUDE.md) earlier in the prompt and
  changing content later, so the cache stops invalidating.

  Lost ≈ $1.32
  src/handlers/payment.rs was read 5 times in session 2c8e0d15 — each
  extra read re-sends its content into context.
  Fix: Keep the file's contents in context (or in CLAUDE.md) instead of
  re-reading it, or Grep for just the lines you need.

  Lost ≈ $9.60
  CLAUDE.md is 412 lines (~3921 tokens) and gets re-read on every turn —
  at your current pace that's ≈$11.20/month just to keep it in context.
  Fix: Trim CLAUDE.md to ~200 lines — remove generic advice, keep only
  project-specific rules.
```

> [!TIP]
> Every finding above is exactly three lines: **a dollar amount, a reason, and a
> fix.** No dashboards to open, no charts to interpret — you get the number and
> the diff to make.

<br>

## ⚡ Quick Start

```bash
# Build & install from source — no crates.io publish yet, this is the
# only command that works today (see Installation below for why)
cargo install --git https://github.com/ChevvyOkK/contextguard contextguard

# Run an audit against every local session you have
contextguard

# Just the last 7 days
contextguard --days 7

# Russian output
contextguard --lang ru

# Compact Markdown, for pasting into Slack or a GitHub PR comment
contextguard --format markdown

# Opt-in: push aggregated daily numbers (no code, no prompts) to your dashboard
contextguard --push --api-url https://your-dashboard.example.com --api-key cg_live_...
```

<br>

## 📦 Installation

There is **no `crates.io` publish and no GitHub Release binary yet** — cutting
the first tagged release (`v0.1.0`) is a deliberate, visible action the
maintainer hasn't pulled the trigger on. Until then, there are exactly two
ways to get `contextguard` on your machine, and both work today:

<table>
<tr>
<td width="50%" valign="top">

**Via Cargo (recommended)**

```bash
cargo install --git \
  https://github.com/ChevvyOkK/contextguard \
  contextguard
```

Needs a Rust toolchain (`rustup.rs`), MSRV **1.85+** (edition 2024).

</td>
<td width="50%" valign="top">

**From source**

```bash
git clone https://github.com/ChevvyOkK/contextguard
cd contextguard
cargo build --release
./target/release/contextguard
```

</td>
</tr>
</table>

> [!NOTE]
> `.github/workflows/release.yml` already cross-compiles Linux, macOS
> (x64 + arm64), and Windows binaries and attaches them to a GitHub Release —
> it just hasn't fired yet, because it only triggers on a `v*` tag push.
> Once one ships, prebuilt binaries land here without any change to the
> commands above.

<br>

## 🧠 The six detectors

Six independent algorithms in [`src/optimize.rs`](src/optimize.rs) turn raw
per-turn usage into concrete, three-line findings — a dollar loss, a reason,
and a fix. Every one of them ships with unit tests that assert both the
positive and negative case (it fires when it should, and stays quiet on
healthy sessions).

| # | Detector | Fires when | Typical fix |
|---|---|---|---|
| 1 | **Cache Churn Detector** | Cache-write tokens outweigh cache-read tokens on turns 2+ (**>25%** of the mix) — the cache is being rebuilt, not reused | Put stable content (files, `CLAUDE.md`) earlier in the prompt, changing content later |
| 2 | **Re-Read Watcher** | The same file gets `Read` **3 or more times** in one session | Keep it in context, or `Grep` for just the lines you need |
| 3 | **CLAUDE.md Amortizer** | `CLAUDE.md` exceeds the 200-line guideline and gets re-read every turn | Trim it to project-specific rules; projects the monthly $ cost of *not* doing so |
| 4 | **Burn-Rate Watch** | A session's `$/hour` blows past the **p95** of your own other sessions | Flags the session so you can see what made it spike |
| 5 | **Context Growth Advisor** | Cache-read context grows **1.8×+** with no `/compact` in between | Points at the turn where compaction would've paid off |
| 6 | **Model-Mismatch Detector** | Opus is used on a short-output, `Edit`/`Write`-only turn — no orchestration that would justify it | Route simple edits to Sonnet instead |

<br>

## 🤖 CI Integration — post costs on every PR

There's no built-in `pr-cost` subcommand (yet) — `--format markdown` already
produces exactly what a PR comment needs, so wiring it up is a few lines of
YAML using a community action to post/update the comment:

```yaml
# .github/workflows/pr-cost.yml
name: ContextGuard PR Cost
on: pull_request

jobs:
  cost:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install --git https://github.com/ChevvyOkK/contextguard contextguard
      - run: contextguard --format markdown > cost-report.md
      - uses: marocchino/sticky-pull-request-comment@v2
        with:
          path: cost-report.md
```

<br>

## ⚙️ Flags reference

| Flag | Value | Default | Does |
|---|---|---|---|
| `--days <N>` | integer | all sessions | Only consider sessions from the last `N` days |
| `--claude-md <PATH>` | path | `./CLAUDE.md` if present | Analyze a specific `CLAUDE.md` instead of auto-detecting one in the cwd |
| `--format <FORMAT>` | `text` \| `markdown` | `text` | `text` is the full interactive report; `markdown` is a compact top-3-findings summary for Slack/PRs |
| `--lang <LANG>` | `en` \| `ru` | `en`, or `$CONTEXTGUARD_LANG` | Output language |
| `--push` | flag | off | Push aggregated daily snapshots (numeric totals only) to the dashboard |
| `--api-url <URL>` | url | `$CONTEXTGUARD_API_URL` | Dashboard API base URL, used with `--push` |
| `--api-key <KEY>` | string | `$CONTEXTGUARD_API_KEY` | Dashboard API key, used with `--push` |

`--push` sends **one row per calendar day**: token counts by category,
session count, and estimated cost, plus the companion
[plugin](https://github.com/ChevvyOkK/contextguard-plugin)'s tokens-saved
figure for that day. Never code, prompts, tool names, or file paths — see
[`contextguard-api`](https://github.com/ChevvyOkK/contextguard-api) for the
exact schema this is validated against on the way in.

<br>

## What it deliberately doesn't do

- No network call of any kind unless you pass `--push`.
- No telemetry, no analytics, no phone-home on startup.
- No modification of your session files, `CLAUDE.md`, or anything else on disk — it only reads.

<br>

## Related

- [`contextguard-plugin`](https://github.com/ChevvyOkK/contextguard-plugin) — a Claude Code plugin that actively trims waste in real time (truncates noisy Bash output, caps unbounded Grep results)
- [`contextguard-api`](https://github.com/ChevvyOkK/contextguard-api) — the API `--push` talks to
- [contextguard-web.vercel.app](https://contextguard-web.vercel.app) — hosted team dashboard + a browser-only transcript analyzer that needs no install at all

<br>

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
