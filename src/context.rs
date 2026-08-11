//! What is actually occupying the context window, and what that costs.
//!
//! Every other tool in this space breaks spend down by session, model or
//! project. None of them answers the question a developer actually has when
//! the bill arrives: *what is in the 200k I am paying to re-send on every
//! single request?*
//!
//! Two numbers matter and they are not the same:
//!
//! - **Occupancy** — how much of the window a category holds. Answers "what
//!   is in there".
//! - **Amortised cost** — size multiplied by the number of requests that
//!   still had to carry it. Answers "what is it costing me", and it is the
//!   one that makes a 3k CLAUDE.md matter more than a 40k tool result that
//!   arrived on the last turn.
//!
//! ## What is measured and what is estimated
//!
//! Measured, from the transcript: tool results by tool, tool arguments,
//! conversation text, thinking, and everything Claude Code injects on its own
//! account (task reminders, skill and agent listings, hook output, imported
//! memory files).
//!
//! Estimated, because it is not in the transcript at all: the system prompt
//! and the tool schemas. Neither is ever written to disk. What we can do is
//! subtract the visible content from the first response's own prompt-token
//! count — the API told us how big the whole prompt was, so the remainder is
//! the fixed prefix. That residual is reported as an estimate and labelled as
//! one, never folded silently into a measured total.
//!
//! ## What cannot be done here, and why
//!
//! Splitting that residual into "system prompt" versus "the schemas for MCP
//! server X" is not possible from transcripts. Tool definitions are sent to
//! the API, never logged. Naming a per-server figure would require launching
//! each server and asking it for its tool list, which is a different
//! mechanism than reading files off disk. What this module can honestly say
//! is how large the whole fixed prefix is, and which configured MCP servers
//! were never called — the actionable half of the same question.

use std::collections::{HashMap, HashSet};

use crate::pricing::PricingTable;
use crate::session::{ContextKind, SessionStats};

/// Chars per token. Matches src/claude_md.rs so two parts of the same report
/// cannot disagree about the size of the same file.
pub fn approx_tokens(chars: usize) -> u64 {
    (chars as f64 / 4.0).ceil() as u64
}

/// How a figure was arrived at. Rendered next to it, so a reader never has to
/// guess which numbers came from the transcript and which came from
/// subtraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    Measured,
    Estimated,
}

#[derive(Debug, Clone)]
pub struct CategoryUsage {
    pub label: String,
    /// Tokens the category occupies in the window.
    pub tokens: u64,
    /// Tokens re-sent because of it: size times the requests that followed
    /// its arrival. This is what it costs, as opposed to what it weighs.
    pub carried_tokens: u64,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct RepeatedRead {
    pub path: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct McpServerUsage {
    pub server: String,
    pub calls: u64,
}

#[derive(Debug)]
pub struct ContextAudit {
    pub sessions: usize,
    /// Categories, largest occupancy first.
    pub categories: Vec<CategoryUsage>,
    pub total_tokens: u64,
    /// System prompt plus tool schemas, by subtraction. None when no session
    /// had both a first-response token count and content to subtract.
    pub fixed_prefix_tokens: Option<u64>,
    pub repeated_reads: Vec<RepeatedRead>,
    pub mcp_servers: Vec<McpServerUsage>,
    /// Cost of everything carried, at the blended rate of the sessions read.
    pub carried_cost_usd: f64,
}

/// Human label for a context entry, collapsing the long tail so the output
/// stays readable. Tool results keep their tool name because that is the
/// actionable part; attachments keep their type for the same reason.
fn label_for(kind: &ContextKind) -> String {
    match kind {
        ContextKind::ToolResult(Some(name)) => format!("tool results · {name}"),
        ContextKind::ToolResult(None) => "tool results · unattributed".to_string(),
        ContextKind::ToolUse(name) => format!("tool calls · {name}"),
        ContextKind::Attachment(kind) => format!("injected · {kind}"),
        ContextKind::UserText => "your messages".to_string(),
        ContextKind::AssistantText => "assistant replies".to_string(),
        ContextKind::Thinking => "thinking".to_string(),
    }
}

/// MCP tools are named `mcp__<server>__<tool>`, which is the only place a
/// server name appears in a transcript.
fn mcp_server_of(tool_name: &str) -> Option<&str> {
    tool_name.strip_prefix("mcp__")?.split("__").next()
}

/// Tokens a session's first response spent on content we cannot see.
///
/// The API reports how many prompt tokens the first request carried. Subtract
/// what the transcript shows was in it and the remainder is the system prompt
/// and the tool schemas. Returns None when the arithmetic would be
/// meaningless — no token count, or visible content already exceeding the
/// reported prompt, which means the estimator drifted and a negative
/// "estimate" would be worse than none.
fn fixed_prefix_of(session: &SessionStats) -> Option<u64> {
    let prompt_tokens = session.first_prompt_tokens?;
    let visible: usize = session
        .context_entries
        .iter()
        .filter(|e| e.turn_index == 0)
        .map(|e| e.chars)
        .sum();
    prompt_tokens.checked_sub(approx_tokens(visible)).filter(|t| *t > 0)
}

/// Rows worth printing before the tail stops being informative.
const MAX_ROWS: usize = 12;
/// A category smaller than this is noise unless it fits in MAX_ROWS anyway.
const MIN_SHARE: f64 = 0.01;

/// Folds the long tail into one row.
///
/// A real transcript produces forty-odd categories, most of them a few
/// hundred tokens: one row per tool that was ever called, times two for calls
/// and results. Printing them all buries the three that matter. Totals are
/// preserved exactly — the tail is summed, not dropped — so the percentages
/// still add up.
fn collapse_tail(categories: Vec<CategoryUsage>, total_tokens: u64) -> Vec<CategoryUsage> {
    let keep = |index: usize, c: &CategoryUsage| {
        index < MAX_ROWS
            || c.tokens as f64 / total_tokens.max(1) as f64 >= MIN_SHARE
            // Never fold away the one figure that carries a provenance
            // caveat: buried inside "everything else" it would read as
            // measured.
            || c.provenance == Provenance::Estimated
    };

    let (kept, tail): (Vec<_>, Vec<_>) = categories
        .into_iter()
        .enumerate()
        .partition(|(i, c)| keep(*i, c));

    let mut kept: Vec<CategoryUsage> = kept.into_iter().map(|(_, c)| c).collect();
    if tail.len() > 1 {
        kept.push(CategoryUsage {
            label: format!("everything else ({} categories)", tail.len()),
            tokens: tail.iter().map(|(_, c)| c.tokens).sum(),
            carried_tokens: tail.iter().map(|(_, c)| c.carried_tokens).sum(),
            provenance: Provenance::Measured,
        });
    } else {
        kept.extend(tail.into_iter().map(|(_, c)| c));
    }
    kept
}

pub fn audit(sessions: &[SessionStats], pricing: &PricingTable) -> ContextAudit {
    let mut occupancy: HashMap<String, u64> = HashMap::new();
    let mut carried: HashMap<String, u64> = HashMap::new();
    let mut reads: HashMap<String, usize> = HashMap::new();
    let mut mcp_calls: HashMap<String, u64> = HashMap::new();
    let mut prefixes: Vec<u64> = Vec::new();
    let mut counted_sessions = 0usize;

    for session in sessions {
        if session.turns.is_empty() {
            continue;
        }
        counted_sessions += 1;
        let total_turns = session.turns.len();

        for entry in &session.context_entries {
            let tokens = approx_tokens(entry.chars);
            if tokens == 0 {
                continue;
            }
            let label = label_for(&entry.kind);
            *occupancy.entry(label.clone()).or_insert(0) += tokens;

            // Requests that still had to carry this, counting the one it
            // arrived on. An entry on the final turn is carried once.
            let carries = total_turns.saturating_sub(entry.turn_index).max(1) as u64;
            *carried.entry(label).or_insert(0) += tokens * carries;
        }

        for call in &session.tool_call_log {
            if let Some(server) = mcp_server_of(&call.name) {
                *mcp_calls.entry(server.to_string()).or_insert(0) += 1;
            }
        }

        // Re-reads are per session: the same file read in two sessions is two
        // legitimate reads, only a repeat within one costs anything extra.
        let mut per_session: HashMap<&str, usize> = HashMap::new();
        for call in &session.tool_call_log {
            if call.name != "Read" {
                continue;
            }
            if let Some(path) = &call.file_path {
                *per_session.entry(path.as_str()).or_insert(0) += 1;
            }
        }
        for (path, count) in per_session {
            if count > 1 {
                *reads.entry(path.to_string()).or_insert(0) += count;
            }
        }

        if let Some(prefix) = fixed_prefix_of(session) {
            prefixes.push(prefix);
        }
    }

    let mut categories: Vec<CategoryUsage> = occupancy
        .into_iter()
        .map(|(label, tokens)| CategoryUsage {
            carried_tokens: carried.get(&label).copied().unwrap_or(tokens),
            label,
            tokens,
            provenance: Provenance::Measured,
        })
        .collect();

    // The median rather than the mean: a single session started with an
    // unusual tool set should not drag the figure for all the others.
    let fixed_prefix_tokens = if prefixes.is_empty() {
        None
    } else {
        prefixes.sort_unstable();
        Some(prefixes[prefixes.len() / 2])
    };

    if let Some(prefix) = fixed_prefix_tokens {
        // Carried on every request of every session, which is precisely what
        // makes it worth knowing about.
        let total_turns: u64 = sessions.iter().map(|s| s.turns.len() as u64).sum();
        categories.push(CategoryUsage {
            label: "system prompt + tool schemas".to_string(),
            tokens: prefix * counted_sessions.max(1) as u64,
            carried_tokens: prefix * total_turns,
            provenance: Provenance::Estimated,
        });
    }

    categories.sort_by_key(|c| std::cmp::Reverse(c.tokens));
    let total_tokens = categories.iter().map(|c| c.tokens).sum();
    let categories = collapse_tail(categories, total_tokens);

    let mut repeated_reads: Vec<RepeatedRead> = reads
        .into_iter()
        .map(|(path, count)| RepeatedRead { path, count })
        .collect();
    repeated_reads.sort_by_key(|r| std::cmp::Reverse(r.count));
    repeated_reads.truncate(5);

    let mut mcp_servers: Vec<McpServerUsage> = mcp_calls
        .into_iter()
        .map(|(server, calls)| McpServerUsage { server, calls })
        .collect();
    mcp_servers.sort_by_key(|s| std::cmp::Reverse(s.calls));

    // Priced at the blend the sessions actually ran on rather than a fixed
    // tier, and at the cache-read rate: carried context is by definition
    // context that was already sent once.
    let carried_total: u64 = categories.iter().map(|c| c.carried_tokens).sum();
    let carried_cost_usd = cache_read_rate(sessions, pricing) * carried_total as f64 / 1_000_000.0;

    ContextAudit {
        sessions: counted_sessions,
        categories,
        total_tokens,
        fixed_prefix_tokens,
        repeated_reads,
        mcp_servers,
        carried_cost_usd,
    }
}

/// Blended cache-read price across the models these sessions actually used,
/// weighted by turns.
fn cache_read_rate(sessions: &[SessionStats], pricing: &PricingTable) -> f64 {
    let mut total = 0.0;
    let mut turns = 0u64;
    for session in sessions {
        for turn in &session.turns {
            total += pricing.for_model(&turn.model).cache_read_per_mtok;
            turns += 1;
        }
    }
    if turns == 0 { 0.0 } else { total / turns as f64 }
}

/// MCP servers Claude Code is configured to launch, read from its own
/// settings file.
///
/// Both the global list and every per-project list, unioned: the sessions
/// being audited span projects, so narrowing to one project's config would
/// call a server "unused" that another project uses constantly.
///
/// Absent or unreadable config is not an error. It means we cannot say which
/// servers exist, only which ones were called, and the report says less
/// rather than guessing.
pub fn configured_servers() -> Vec<String> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(home.join(".claude.json")) else { return Vec::new() };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };

    let mut names: HashSet<String> = HashSet::new();
    let mut collect = |value: Option<&serde_json::Value>| {
        if let Some(map) = value.and_then(|v| v.as_object()) {
            names.extend(map.keys().cloned());
        }
    };

    collect(root.get("mcpServers"));
    if let Some(projects) = root.get("projects").and_then(|v| v.as_object()) {
        for project in projects.values() {
            collect(project.get("mcpServers"));
        }
    }

    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    names
}

/// Configured servers that never answered a call in the sessions analysed.
///
/// Their schemas sit in the fixed prefix of every request whether or not they
/// are used. How many tokens each one costs is not knowable from a transcript
/// — only that nothing invoked them, which is still the actionable half.
pub fn unused_servers(configured: &[String], audit: &ContextAudit) -> Vec<String> {
    let used: HashSet<&str> = audit.mcp_servers.iter().map(|s| s.server.as_str()).collect();
    configured.iter().filter(|s| !used.contains(s.as_str())).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ContextEntry, Turn, Usage};

    fn session_with(entries: Vec<ContextEntry>, turns: usize) -> SessionStats {
        SessionStats {
            session_id: "s".into(),
            turns: (0..turns)
                .map(|_| Turn { model: "claude-sonnet-5".into(), usage: Usage::default(), timestamp: None })
                .collect(),
            context_entries: entries,
            ..Default::default()
        }
    }

    fn entry(kind: ContextKind, chars: usize, turn_index: usize) -> ContextEntry {
        ContextEntry { turn_index, kind, chars }
    }

    #[test]
    fn groups_tool_results_by_the_tool_that_produced_them() {
        let s = session_with(
            vec![
                entry(ContextKind::ToolResult(Some("Bash".into())), 4000, 0),
                entry(ContextKind::ToolResult(Some("Bash".into())), 4000, 1),
                entry(ContextKind::ToolResult(Some("Read".into())), 400, 1),
            ],
            2,
        );
        let audit = audit(&[s], &PricingTable::defaults());
        let bash = audit.categories.iter().find(|c| c.label.contains("Bash")).unwrap();
        assert_eq!(bash.tokens, 2000, "8000 chars at 4 chars/token");
        let read = audit.categories.iter().find(|c| c.label.contains("Read")).unwrap();
        assert_eq!(read.tokens, 100);
    }

    #[test]
    fn charges_early_arrivals_for_every_request_that_carried_them() {
        // The same 4000 characters cost five times as much arriving on turn 0
        // of a five-turn session as they do arriving on the last turn. That
        // asymmetry is the entire argument for trimming CLAUDE.md.
        let early = session_with(vec![entry(ContextKind::UserText, 4000, 0)], 5);
        let late = session_with(vec![entry(ContextKind::UserText, 4000, 4)], 5);
        let a = audit(&[early], &PricingTable::defaults());
        let b = audit(&[late], &PricingTable::defaults());
        assert_eq!(a.categories[0].tokens, b.categories[0].tokens, "same size");
        assert_eq!(a.categories[0].carried_tokens, 5 * b.categories[0].carried_tokens);
    }

    #[test]
    fn separates_injected_content_from_the_conversation() {
        let s = session_with(
            vec![
                entry(ContextKind::Attachment("task_reminder".into()), 40_000, 0),
                entry(ContextKind::UserText, 400, 0),
            ],
            1,
        );
        let audit = audit(&[s], &PricingTable::defaults());
        let injected = audit.categories.iter().find(|c| c.label.contains("task_reminder")).unwrap();
        assert_eq!(injected.tokens, 10_000);
        // Largest first, so the thing worth acting on leads the report.
        assert_eq!(audit.categories[0].label, injected.label);
    }

    #[test]
    fn estimates_the_prefix_the_transcript_never_contains() {
        let mut s = session_with(vec![entry(ContextKind::UserText, 400, 0)], 1);
        // The API says the first prompt was 30,100 tokens; 100 of them are
        // the visible message, so ~30,000 are prompt and schemas.
        s.first_prompt_tokens = Some(30_100);
        let audit = audit(&[s], &PricingTable::defaults());
        let prefix = audit.categories.iter().find(|c| c.label.contains("system prompt")).unwrap();
        assert_eq!(prefix.provenance, Provenance::Estimated);
        assert_eq!(audit.fixed_prefix_tokens, Some(30_000));
    }

    #[test]
    fn declines_to_estimate_a_prefix_it_cannot_justify() {
        // Visible content larger than the reported prompt means the estimator
        // has drifted. A negative residual dressed up as a figure would be
        // worse than admitting there isn't one.
        let mut s = session_with(vec![entry(ContextKind::UserText, 400_000, 0)], 1);
        s.first_prompt_tokens = Some(1_000);
        let audit = audit(&[s], &PricingTable::defaults());
        assert_eq!(audit.fixed_prefix_tokens, None);
        assert!(audit.categories.iter().all(|c| c.provenance == Provenance::Measured));
    }

    #[test]
    fn counts_a_file_read_twice_in_one_session_but_not_across_two() {
        use crate::session::ToolCall;
        let mut a = session_with(vec![], 2);
        a.tool_call_log = vec![
            ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("/a.rs".into()) },
            ToolCall { turn_index: 1, name: "Read".into(), file_path: Some("/a.rs".into()) },
        ];
        let mut b = session_with(vec![], 1);
        b.tool_call_log =
            vec![ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("/b.rs".into()) }];

        let audit = audit(&[a, b], &PricingTable::defaults());
        assert_eq!(audit.repeated_reads.len(), 1);
        assert_eq!(audit.repeated_reads[0].path, "/a.rs");
        assert_eq!(audit.repeated_reads[0].count, 2);
    }

    #[test]
    fn reads_the_server_name_out_of_an_mcp_tool_call() {
        use crate::session::ToolCall;
        let mut s = session_with(vec![], 1);
        s.tool_call_log = vec![
            ToolCall { turn_index: 0, name: "mcp__figma__get_file".into(), file_path: None },
            ToolCall { turn_index: 0, name: "mcp__figma__list".into(), file_path: None },
            ToolCall { turn_index: 0, name: "Bash".into(), file_path: None },
        ];
        let audit = audit(&[s], &PricingTable::defaults());
        assert_eq!(audit.mcp_servers.len(), 1);
        assert_eq!(audit.mcp_servers[0].server, "figma");
        assert_eq!(audit.mcp_servers[0].calls, 2);

        let unused = unused_servers(&["figma".into(), "notion".into()], &audit);
        assert_eq!(unused, vec!["notion".to_string()]);
    }

    #[test]
    fn folds_the_long_tail_without_losing_a_token() {
        // A real transcript produces one category per tool called, twice
        // over (calls and results). Forty rows of a few hundred tokens each
        // bury the three that matter.
        let mut entries = vec![entry(ContextKind::UserText, 4_000_000, 0)];
        for i in 0..30 {
            entries.push(entry(ContextKind::ToolResult(Some(format!("Tool{i}"))), 40, 0));
        }
        let before: u64 = entries.iter().map(|e| approx_tokens(e.chars)).sum();

        let audit = audit(&[session_with(entries, 1)], &PricingTable::defaults());

        assert!(audit.categories.len() <= MAX_ROWS + 1, "tail folded into one row");
        assert!(audit.categories.iter().any(|c| c.label.starts_with("everything else")));
        assert_eq!(
            audit.categories.iter().map(|c| c.tokens).sum::<u64>(),
            before,
            "folding is a sum, not a truncation",
        );
    }

    #[test]
    fn keeps_a_short_list_intact() {
        let s = session_with(
            vec![
                entry(ContextKind::UserText, 4000, 0),
                entry(ContextKind::AssistantText, 400, 0),
            ],
            1,
        );
        let audit = audit(&[s], &PricingTable::defaults());
        assert_eq!(audit.categories.len(), 2);
        assert!(audit.categories.iter().all(|c| !c.label.starts_with("everything else")));
    }

    #[test]
    fn never_buries_the_estimated_prefix_in_the_tail() {
        // It is the one row carrying a caveat. Inside "everything else" it
        // would read as measured.
        let mut entries = vec![entry(ContextKind::UserText, 4_000_000, 0)];
        for i in 0..30 {
            entries.push(entry(ContextKind::ToolResult(Some(format!("Tool{i}"))), 40, 0));
        }
        let mut s = session_with(entries, 1);
        // Visible content on turn 0 is 4,001,200 chars, so 1,000,300 tokens.
        // A prompt only 50 tokens larger leaves a prefix small enough to fall
        // below both cutoffs — which is the point: nothing but the
        // provenance guard keeps it out of the tail.
        s.first_prompt_tokens = Some(1_000_350);

        let audit = audit(&[s], &PricingTable::defaults());
        let prefix = audit
            .categories
            .iter()
            .find(|c| c.provenance == Provenance::Estimated)
            .expect("estimated row survives the fold");
        assert!(prefix.label.contains("system prompt"));
        assert_eq!(prefix.tokens, 50);
    }

    #[test]
    fn an_empty_history_produces_an_empty_audit_rather_than_a_panic() {
        let audit = audit(&[], &PricingTable::defaults());
        assert_eq!(audit.sessions, 0);
        assert_eq!(audit.total_tokens, 0);
        assert_eq!(audit.carried_cost_usd, 0.0);
        assert!(audit.fixed_prefix_tokens.is_none());
    }
}
