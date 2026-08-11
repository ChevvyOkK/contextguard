<div align="center">

# ContextGuard

**Blazing-fast, local-first CLI in Rust that finds where your Claude Code tokens go —**
**cache churn, dead re-reads, and `CLAUDE.md` bloat, before it hits your invoice.**

[![CI](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml/badge.svg)](https://github.com/ChevvyOkK/contextguard/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-0.4.0-6366f1)](Cargo.toml)
[![Rust MSRV](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)](Cargo.toml)
[![License: MIT OR Apache--2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![i18n](https://img.shields.io/badge/i18n-EN%20%7C%20RU-informational)](src/i18n.rs)
[![GitHub Stars](https://img.shields.io/github/stars/ChevvyOkK/contextguard?style=social)](https://github.com/ChevvyOkK/contextguard/stargazers)

[Quick Start](#-quick-start) · [Key Features](#-the-six-detectors) · [CI/CD](#-ci-integration--comment-the-tokencost-delta-on-every-pr) · [Web Dashboard](https://contextguard-web.vercel.app)

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

**Prebuilt binaries** for Linux (x86_64 + ARM), macOS (Intel + Apple
Silicon), and Windows are attached to every
[GitHub Release](https://github.com/ChevvyOkK/contextguard/releases) —
download the one for your platform and put it on your `PATH`. No
`crates.io` publish yet, so `cargo install` needs `--git` rather than a bare
crate name:

<table>
<tr>
<td width="50%" valign="top">

**Via Cargo**

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

## 🩺 `contextguard lint` — CLAUDE.md diagnostics with a safe autofix

`CLAUDE.md` is resent with *every single request* in every session, so a
line that adds nothing isn't a style nit — it's a permanent tax on every
turn for as long as it stays in the file. `contextguard lint` finds four
kinds of line, in order of how confident an automated tool can be about the
finding:

| Finding | What it means | `--fix` touches it? |
|---|---|---|
| **Boilerplate** | Restates behavior the model already has by default ("write clean code") | ✅ removed |
| **Duplicate** | Identical to an earlier line in the same file | ✅ removed |
| **Stale path** | Names a file (in backticks) no analyzed session ever touched | ⚠️ flagged only — could still be load-bearing even if no tool call shows it |
| **Unused MCP server** | Names a server that's configured but was never called | ⚠️ flagged only |

```text
$ contextguard lint CLAUDE.md --fix

CLAUDE.md lint — CLAUDE.md
412 lines, ~3 900 tokens

  boilerplate        L88   Always write clean code.
                            restates default behavior ("write clean code")
  stale path          L214  See `src/legacy-importer.rs` for details.
                            no analyzed session touched this path

1 of these can be removed automatically with --fix (~6 tokens)

$0.0012 per 1,000 requests at Anthropic's published Sonnet cache-read rate
≈$4.10/mo at the volume observed in the sessions analyzed
--fix would save about $0.02/mo at that volume

--fix will remove 1 line(s):
  - L88   Always write clean code.
    restates default behavior ("write clean code")

Done — 1 line(s) removed, 411 remain.
```

Two different dollar figures, because they come from two different sources
of truth. **Price per 1,000 requests** is deterministic — Anthropic's
published cache-read rate applied to the file's own token count, available
even from a bare CI checkout with zero local session history. **$/month at
your volume** only appears when local session transcripts exist to measure
that volume from; on a machine with none, it says so instead of guessing.

<br>

## 🤖 CI Integration — comment the token/cost delta on every PR

`action.yml` in this repo is a ready-to-use composite GitHub Action. It
installs the right prebuilt binary for the runner, diffs the PR's
`CLAUDE.md` against the base branch, and posts (or updates) one PR comment
with the result — nothing happens on a PR that doesn't touch the file.

```yaml
# .github/workflows/claude-md-lint.yml
name: CLAUDE.md cost check
on: pull_request

permissions:
  pull-requests: write

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0   # needed so the action can diff base vs head
      - uses: ChevvyOkK/contextguard@v0.3.0
```

Comment example:

> **CLAUDE.md changed: base/CLAUDE.md → CLAUDE.md**
>
> +1,800 tokens (from 3,200 to 5,000) on every request
>
> That's about $0.54 per 1,000 requests at Anthropic's published Sonnet
> cache-read rate.
>
> _Run `contextguard lint --compare-to <base> CLAUDE.md` locally to price
> this at your team's actual request volume._

<br>

## 🧭 `contextguard context` — what is actually in the window

Every other tool in this space breaks spend down by session, model or
project. `contextguard context` answers the question that actually matters
when the bill arrives: *what is in the 200k I'm paying to re-send on every
request?* See [`src/context.rs`](src/context.rs) for the full breakdown —
tool results by tool, everything Claude Code injects on its own account
(task reminders, skill listings, hook output), an estimate of the fixed
system-prompt-and-tool-schemas prefix, which configured MCP servers went
unused, and which files got re-read.

```text
$ contextguard context --days 7

What is in your context window
Averaged across 16 session(s) on this machine

  tool results · Bash            238 249 tok   6%  ███████
  your messages                  885 351 tok  21%  ████████████████████████
  system prompt + tool schemas   593 568 tok  14%  ████████████████ estimated
  ...

Re-sent across every request in these sessions: 735 235 344 tokens, about $380.
```

Add `--format json` for a machine-readable version.

<br>

## 💰 `contextguard savings` — what the plugin actually saved

The bash-truncate hook doesn't estimate its savings — it measures the exact
character delta between the tool output Claude Code would otherwise have
received and the smaller version it actually wrote to the transcript,
before either one is ever sent to the API. Nothing here has to guess at a
counterfactual by running anything twice.

What this adds is amortization. The truncated output is what gets cached
and resent on every subsequent turn of that session, not a one-time
saving — a truncation on turn 3 of a 40-turn session is worth far more than
the same truncation on the last turn. `contextguard savings` prices each
one by how many turns were still ahead of it, the same "carried tokens"
idea `context` uses for `CLAUDE.md` and tool results, and falls back to
counting a saving once — a floor, not a guess — when it can't be matched to
a locally-parsed session.

```text
$ contextguard savings

Savings report — August 2026
Saved by the plugin: 34531 tokens ≈ $11.11 (amortized: priced by how many
turns of the session were still ahead of each intervention)
From 11 intervention(s) across 1 session(s) this month
Top source: npm test output truncated — 24531 tokens
Also capped 24 unbounded Grep search(es) this month — no token estimate:
nothing to compare against without running the search twice.
```

The Grep-cap hook is reported separately and never priced: it fires
*before* the search runs, so there's nothing yet to measure a delta
against — see `cap-grep-limit.js`'s own comment on this. `--format
markdown` for pasting into a report; requires a plugin build recent enough
to log a `session_id` and command label for full amortization (older
entries are still counted, just at the floor).

<br>

## 🚨 `contextguard budget` — a local spend threshold

Fully local: sums cost from the session transcripts already on disk for
today or the current calendar month, compares it to `--max`, and exits
non-zero if it's crossed — so it can gate a script or a pre-commit-style
check without an account or a network call.

```bash
contextguard budget --max 50 --period month
# exits 1 and prints in red if $50 has been crossed this calendar month,
# exits 0 and prints in green otherwise
```

Add `--webhook-url` (or set `$CONTEXTGUARD_BUDGET_WEBHOOK`) to also post a
Slack/Discord-compatible message when the threshold is crossed — the same
payload shape the dashboard's own team-level budget alerts send, just
usable without ever pushing data anywhere. A delivery failure is reported
but doesn't change the exit code: the budget verdict is real regardless of
whether the notification made it out.

This is a different mechanism from the dashboard's team-level budget
alerts (which need an account and `--push`'d data, and fire from the
server on every ingest) — this one works for someone who has never pushed
anything anywhere.

<br>

## ⚙️ Flags reference

Global flags apply to every mode:

| Flag | Value | Default | Does |
|---|---|---|---|
| `--days <N>` | integer | all sessions | Only consider sessions from the last `N` days |
| `--lang <LANG>` | `en` \| `ru` | `en`, or `$CONTEXTGUARD_LANG` | Output language |

Headline report (no subcommand):

| Flag | Value | Default | Does |
|---|---|---|---|
| `--claude-md <PATH>` | path | `./CLAUDE.md` if present | Analyze a specific `CLAUDE.md` instead of auto-detecting one in the cwd |
| `--format <FORMAT>` | `text` \| `markdown` | `text` | `text` is the full interactive report; `markdown` is a compact top-3-findings summary for Slack/PRs |
| `--push` | flag | off | Push aggregated daily snapshots (numeric totals only) to the dashboard |
| `--api-url <URL>` | url | `$CONTEXTGUARD_API_URL` | Dashboard API base URL, used with `--push` |
| `--api-key <KEY>` | string | `$CONTEXTGUARD_API_KEY` | Dashboard API key, used with `--push` |

`contextguard context [--format text|json]`:

| Flag | Value | Default | Does |
|---|---|---|---|
| `--format <FORMAT>` | `text` \| `json` | `text` | `json` for scripting |

`contextguard lint [PATH] [--fix] [--format text|markdown] [--compare-to <FILE>]`:

| Flag | Value | Default | Does |
|---|---|---|---|
| `PATH` | path | `./CLAUDE.md` | Which file to lint |
| `--fix` | flag | off | Remove boilerplate/duplicate lines and write the file. Always prints what changed first. |
| `--format <FORMAT>` | `text` \| `markdown` | `text` | `markdown` is what the PR-bot posts |
| `--compare-to <FILE>` | path | — | Report the token/cost delta between `FILE` (baseline) and `PATH` instead of a full lint. A missing baseline is treated as empty, not an error. |

`contextguard savings [--format text|markdown]`:

| Flag | Value | Default | Does |
|---|---|---|---|
| `--format <FORMAT>` | `text` \| `markdown` | `text` | `markdown` for pasting into a report |

`contextguard budget --max <USD> [--period daily|monthly] [--webhook-url <URL>]`:

| Flag | Value | Default | Does |
|---|---|---|---|
| `--max <USD>` | number | — (required) | Threshold; exits 1 if crossed |
| `--period <PERIOD>` | `daily` \| `monthly` | `monthly` | Which window to sum spend over |
| `--webhook-url <URL>` | url | `$CONTEXTGUARD_BUDGET_WEBHOOK` | Slack/Discord-compatible POST when crossed |

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
- No modification of anything on disk, with one explicit exception: `contextguard lint --fix` writes to the `CLAUDE.md` you point it at, after printing exactly what it's about to remove. Every other mode, including plain `lint`, only reads.

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
