# ContextGuard

A CLI that audits how many tokens your Claude Code sessions actually burn —
and why — by reading the session transcripts Claude Code already writes to
your own disk. No signup, no upload, no network call.

## Why this exists

Claude Code (and tools like it) bill by the token, but nothing in the
day-to-day workflow tells you where those tokens went. Was it a few huge
`Read`s of files you only needed ten lines from? A bloated `CLAUDE.md` that
gets sent on every single turn, cache-write cost and all? A session that
just never triggered prompt-cache reuse because the stable and the
changing parts of the context kept getting interleaved? You only find out
at the end of the month, as a total, with no breakdown.

`ContextGuard` reads the JSONL transcripts Claude Code already stores under
`~/.claude/projects/` on your machine, aggregates the real `usage` blocks
(input, output, cache-write, cache-read tokens) per session, applies a
published per-model pricing table to estimate real cost, and flags the two
most common sources of waste: low cache-hit rate, and an oversized or
generic `CLAUDE.md`.

## Usage

```bash
# Audit every local session ever recorded
contextguard

# Only the last 7 days
contextguard --days 7

# Point it at a specific CLAUDE.md instead of auto-detecting one in cwd
contextguard --claude-md ./CLAUDE.md

# Output in Russian instead of the English default
contextguard --lang ru
# or: CONTEXTGUARD_LANG=ru contextguard

# Push aggregated daily snapshots to the hosted dashboard (opt-in only —
# nothing is sent anywhere without --push)
contextguard --push --api-url https://your-dashboard.example.com --api-key cg_...
# or: CONTEXTGUARD_API_URL=... CONTEXTGUARD_API_KEY=... contextguard --push
```

`--push` sends one row per calendar day covered by your local sessions —
token counts by category, session count, and estimated cost, plus the
plugin's tokens-saved figure for today only. It never sends code, prompts,
tool names, or file paths; see [`contextguard-api`](https://github.com/ChevvyOkK/contextguard-api)
for the exact schema this is validated against on the way in.

Output is a terminal report: total sessions, token breakdown by category,
estimated cost, cache-hit efficiency, the most expensive individual
sessions, the most-called tools, and a `CLAUDE.md` bloat check if one is
found — plus concrete suggestions where something looks off. If the
companion [ContextGuard plugin](https://github.com/ChevvyOkK/contextguard-plugin)
is installed, its local intervention log is picked up automatically and
shown at the top of the report.

## What it deliberately doesn't do

- **Never reads or uploads your code or conversation content.** It parses
  only the numeric `usage` fields and `tool_use` block names out of each
  transcript line — the actual prompt/response text is never touched,
  logged, or sent anywhere. There is no network call in this tool at all.
- **Cost figures are estimates, not an invoice.** The pricing table is
  hardcoded from published API rates, not a live price feed, and doesn't
  know about your specific plan or any volume pricing. Treat it as good
  enough to spot trends and waste, not to reconcile a bill.
- **The `CLAUDE.md` check is a rule-based heuristic**, not an LLM judgment
  call — line-count threshold plus a short list of generic boilerplate
  phrases. It'll miss subtler bloat and occasionally flag something that's
  actually fine. That's intentional: a transparent, predictable rule beats
  an opaque "AI says your file is bad."

## Building from source

```bash
cargo build --release
```

Needs Rust 2024 edition (stable). No non-Rust dependencies.

> On Windows with MinGW, if your checkout path contains non-ASCII
> characters, the linker may fail. Work around it locally (not committed —
> it's a machine-specific path) with a `.cargo/config.toml`:
> ```toml
> [build]
> target-dir = "C:/some/ascii/only/path"
> ```

## Tests

```bash
cargo test
```
