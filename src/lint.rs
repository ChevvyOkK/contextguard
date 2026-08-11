//! `contextguard lint` — CLAUDE.md diagnostics with a safe autofix.
//!
//! CLAUDE.md is resent with every single request in every session, so a
//! line that adds nothing is not a style nit — it is a permanent tax on
//! every turn for as long as it stays in the file. This finds four kinds of
//! line, in order of how confident an automated tool can be about the
//! finding:
//!
//! 1. **Boilerplate** — restates behavior the model already has by default
//!    ("write clean code"). Safe to remove outright.
//! 2. **Duplicate** — identical to an earlier line in the same file. Safe
//!    to remove outright.
//! 3. **Stale path** — names a file (in backticks) that no analyzed
//!    session ever touched. A signal, not a verdict: the model may have
//!    followed the instruction without that showing up as a Read/Edit/
//!    Write call, so this is reported and never auto-removed.
//! 4. **Unused MCP server** — names a server (see src/context.rs) that is
//!    configured but was never called in the analyzed sessions. Same
//!    caveat as above.
//!
//! Only categories 1 and 2 are ever touched by `--fix`.
//!
//! This module holds data and logic only — no printing, no i18n. Rendering
//! (including translating a `Reason` into a sentence) lives in report.rs,
//! the same split context.rs uses for `Provenance`.
//!
//! ## The dollar figure, and where it does and doesn't come from
//!
//! Two different numbers, because they come from two different sources of
//! truth:
//!
//! - **Price per 1,000 requests**, at Anthropic's published cache-read
//!   rate: deterministic, needs nothing but the file's token count. Always
//!   available, including from a bare CI checkout with no local session
//!   history.
//! - **$/month at your volume**: only when local session transcripts are
//!   available to measure that volume from. A CI runner checking out a
//!   fresh clone has none of these — and printing a monthly figure built on
//!   an invented request rate is exactly the mistake Phase 2 of this
//!   project existed to fix. When there is nothing to measure a rate from,
//!   this reports `None` rather than a guess.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::context;
use crate::i18n::{self, Lang};
use crate::pricing::PricingTable;
use crate::session::SessionStats;
use crate::timeutil;

const MTOK: f64 = 1_000_000.0;

/// Phrases that restate default model behavior and add no project-specific
/// information. Matched as a case-insensitive substring.
const GENERIC_BOILERPLATE: &[&str] = &[
    "write clean code",
    "follow best practices",
    "be helpful",
    "write good code",
    "use meaningful variable names",
    "add comments to explain",
    "write readable code",
    "follow the dry principle",
    "keep it simple",
    "write maintainable code",
    "use git for version control",
    "write tests for your code",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    Boilerplate,
    Duplicate,
    StalePath,
    UnusedMcpServer,
}

impl FindingKind {
    /// Whether `--fix` is allowed to delete this line outright. Only the two
    /// findings with no false-positive cost: boilerplate and an exact
    /// repeat can't be "actually load-bearing" the way a path or server
    /// reference might turn out to be.
    pub fn auto_fixable(self) -> bool {
        matches!(self, FindingKind::Boilerplate | FindingKind::Duplicate)
    }
}

/// The specific evidence behind a finding. Kept structured rather than
/// pre-formatted so report.rs can render it in either language.
#[derive(Debug, Clone)]
pub enum Reason {
    Boilerplate { phrase: &'static str },
    /// The line number this text first appeared at.
    Duplicate { origin_line: usize },
    StalePath { token: String },
    UnusedMcpServer { server: String },
}

impl Reason {
    pub fn kind(&self) -> FindingKind {
        match self {
            Reason::Boilerplate { .. } => FindingKind::Boilerplate,
            Reason::Duplicate { .. } => FindingKind::Duplicate,
            Reason::StalePath { .. } => FindingKind::StalePath,
            Reason::UnusedMcpServer { .. } => FindingKind::UnusedMcpServer,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    /// 1-based, matching what an editor shows.
    pub line: usize,
    pub text: String,
    pub reason: Reason,
}

impl Finding {
    pub fn kind(&self) -> FindingKind {
        self.reason.kind()
    }
}

#[derive(Debug)]
pub struct LintReport {
    pub path: String,
    pub line_count: usize,
    pub total_tokens: u64,
    pub findings: Vec<Finding>,
    /// Tokens in lines flagged as auto-fixable — what `--fix` would remove.
    pub fixable_tokens: u64,
    /// USD per 1,000 requests at Anthropic's published Sonnet cache-read
    /// rate. Deterministic; needs nothing but the token count.
    pub cost_per_1k_requests_usd: f64,
    /// USD/month at the volume observed in the sessions analyzed. `None`
    /// when there is no local session data to measure a volume from.
    pub monthly_cost_usd: Option<f64>,
    pub fixable_monthly_savings_usd: Option<f64>,
}

pub fn analyze(path: &Path, sessions: &[SessionStats], lang: Lang) -> Result<LintReport, String> {
    let content = std::fs::read_to_string(path).map_err(|e| i18n::err_read_file(lang, &format!("{path:?}"), &e.to_string()))?;
    Ok(lint(&path.display().to_string(), &content, sessions))
}

/// Same as [`analyze`], but a missing file is treated as an empty baseline
/// rather than an error — exactly right for `--compare-to` against a file
/// this change adds for the first time.
pub fn analyze_optional(path: &Path, sessions: &[SessionStats], lang: Lang) -> Result<LintReport, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(lint(&path.display().to_string(), &content, sessions)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(lint(&path.display().to_string(), "", sessions)),
        Err(e) => Err(i18n::err_read_file(lang, &format!("{path:?}"), &e.to_string())),
    }
}

pub fn lint(path_label: &str, content: &str, sessions: &[SessionStats]) -> LintReport {
    let lines: Vec<&str> = content.lines().collect();
    let mut findings = Vec::new();
    // Line text -> the line number it first appeared at, so a duplicate
    // finding can point back at its origin instead of just saying "seen
    // before somewhere."
    let mut first_seen: HashMap<&str, usize> = HashMap::new();

    let have_session_data = !sessions.is_empty();

    // Every path any analyzed session actually touched. Backslash and
    // forward-slash are normalized together — CLAUDE.md conventionally
    // writes `src/foo.rs`, transcripts on Windows record `C:\...\src\foo.rs`
    // — so a naive substring check would call every real path stale.
    let touched_paths: Vec<String> =
        sessions.iter().flat_map(|s| s.tool_call_log.iter()).filter_map(|c| c.file_path.as_deref()).map(normalize_path).collect();

    let configured_servers = context::configured_servers();
    let unused_servers: HashSet<String> = if have_session_data {
        let audit = context::audit(sessions, &PricingTable::defaults());
        context::unused_servers(&configured_servers, &audit).into_iter().collect()
    } else {
        HashSet::new()
    };

    for (i, raw_line) in lines.iter().enumerate() {
        let line_no = i + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();

        if let Some(phrase) = GENERIC_BOILERPLATE.iter().find(|p| lower.contains(*p)) {
            findings.push(Finding { line: line_no, text: trimmed.to_string(), reason: Reason::Boilerplate { phrase } });
            continue; // one finding per line
        }

        match first_seen.get(trimmed) {
            Some(&origin) => {
                findings.push(Finding { line: line_no, text: trimmed.to_string(), reason: Reason::Duplicate { origin_line: origin } });
                continue;
            }
            None => {
                first_seen.insert(trimmed, line_no);
            }
        }

        if have_session_data {
            let stale = path_like_tokens(trimmed).into_iter().find(|t| !touched_paths.iter().any(|p| paths_match(p, t)));
            if let Some(token) = stale {
                findings.push(Finding { line: line_no, text: trimmed.to_string(), reason: Reason::StalePath { token } });
                continue;
            }

            let named_unused = configured_servers.iter().find(|s| unused_servers.contains(s.as_str()) && word_boundary_contains(&lower, &s.to_ascii_lowercase()));
            if let Some(server) = named_unused {
                findings.push(Finding {
                    line: line_no,
                    text: trimmed.to_string(),
                    reason: Reason::UnusedMcpServer { server: server.clone() },
                });
            }
        }
    }

    let total_tokens = context::approx_tokens(content.chars().count());
    let fixable_tokens: u64 =
        findings.iter().filter(|f| f.kind().auto_fixable()).map(|f| context::approx_tokens(f.text.chars().count())).sum();

    let sonnet_cache_read = PricingTable::defaults().for_model("claude-sonnet-5").cache_read_per_mtok;
    let cost_per_1k_requests_usd = total_tokens as f64 / MTOK * sonnet_cache_read * 1000.0;

    let (monthly_cost_usd, fixable_monthly_savings_usd) = monthly_estimate(sessions, total_tokens, fixable_tokens);

    LintReport {
        path: path_label.to_string(),
        line_count: lines.len(),
        total_tokens,
        findings,
        fixable_tokens,
        cost_per_1k_requests_usd,
        monthly_cost_usd,
        fixable_monthly_savings_usd,
    }
}

/// Removes every auto-fixable finding's line and returns the new content
/// plus which findings were actually removed. Pure — the caller decides
/// whether and how to write the result.
pub fn apply_fixes(content: &str, report: &LintReport) -> (String, Vec<Finding>) {
    let remove: HashSet<usize> = report.findings.iter().filter(|f| f.kind().auto_fixable()).map(|f| f.line).collect();
    if remove.is_empty() {
        return (content.to_string(), Vec::new());
    }

    let removed: Vec<Finding> = report
        .findings
        .iter()
        .filter(|f| remove.contains(&f.line))
        .map(|f| Finding { line: f.line, text: f.text.clone(), reason: f.reason.clone() })
        .collect();

    let kept: Vec<&str> = content.lines().enumerate().filter(|(i, _)| !remove.contains(&(i + 1))).map(|(_, l)| l).collect();
    let mut new_content = kept.join("\n");
    if content.ends_with('\n') && !kept.is_empty() {
        new_content.push('\n');
    }
    (new_content, removed)
}

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/")
}

/// Whether a touched path and a path-like token from CLAUDE.md refer to the
/// same file. Neither is anchored to the other's root (one is absolute, one
/// is typically project-relative), so a suffix match in either direction is
/// what "the same file" actually looks like here.
fn paths_match(touched: &str, mentioned: &str) -> bool {
    let touched = touched.to_ascii_lowercase();
    let mentioned = normalize_path(mentioned).to_ascii_lowercase();
    touched.ends_with(&mentioned) || mentioned.ends_with(&touched)
}

/// Extracts backtick-quoted tokens that look like file paths: a path
/// separator, a plausible trailing extension. Deliberately narrow — only
/// spans someone already set off as code get checked, so a prose sentence
/// with a slash in it (a date, a fraction) is never mistaken for a path.
/// This means real paths written without backticks go unchecked; that is an
/// under-report, not an over-report, which is the direction to err in for a
/// finding this hard to be certain about.
fn path_like_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find('`') else { break };
        let token = &after_open[..end];
        if looks_like_path(token) {
            out.push(token.to_string());
        }
        rest = &after_open[end + 1..];
    }
    out
}

fn looks_like_path(token: &str) -> bool {
    if token.is_empty() || token.contains(char::is_whitespace) {
        return false;
    }
    if !(token.contains('/') || token.contains('\\')) {
        return false;
    }
    match token.rsplit_once('.') {
        Some((_, ext)) => !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

/// Whether `needle` appears in `haystack` at a word boundary — bare
/// `contains` would match "notion" inside "notional", which is a real word
/// with nothing to do with the MCP server of the same name.
fn word_boundary_contains(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = haystack[..abs].chars().next_back().is_none_or(|c| !is_word_char(c));
        let after_ok = haystack[abs + needle.len()..].chars().next().is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Observed days spanned by the session timestamps, floored at one day so a
/// single burst of activity doesn't divide by a fraction and produce an
/// absurd extrapolated rate.
fn observed_days(sessions: &[SessionStats]) -> f64 {
    let timestamps: Vec<f64> =
        sessions.iter().flat_map(|s| &s.turns).filter_map(|t| t.timestamp.as_deref()).filter_map(timeutil::parse_epoch_seconds).collect();
    if timestamps.len() < 2 {
        return 1.0;
    }
    let min = timestamps.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = timestamps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    ((max - min) / 86_400.0).max(1.0)
}

/// Blended cache-read price across the models these sessions actually used,
/// weighted by turns — mirrors src/context.rs's cache_read_rate and
/// src/optimize.rs's CLAUDE.md amortization, since all three are pricing the
/// same thing: content resent on every subsequent turn after it first
/// enters the cache.
fn blended_cache_read_rate(sessions: &[SessionStats], pricing: &PricingTable) -> f64 {
    let mut total = 0.0;
    let mut turns = 0u64;
    for session in sessions {
        for turn in &session.turns {
            total += pricing.for_model(&turn.model).cache_read_per_mtok;
            turns += 1;
        }
    }
    if turns == 0 {
        0.0
    } else {
        total / turns as f64
    }
}

fn monthly_estimate(sessions: &[SessionStats], total_tokens: u64, fixable_tokens: u64) -> (Option<f64>, Option<f64>) {
    let total_turns: u64 = sessions.iter().map(|s| s.turns.len() as u64).sum();
    if total_turns == 0 {
        return (None, None);
    }
    let monthly_turns = total_turns as f64 / observed_days(sessions) * 30.0;
    let pricing = PricingTable::defaults();
    let rate = blended_cache_read_rate(sessions, &pricing);
    let cost = |tokens: u64| tokens as f64 / MTOK * rate * monthly_turns;
    (Some(cost(total_tokens)), Some(cost(fixable_tokens)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ToolCall, Turn, Usage};

    fn session_with_turns(n: usize) -> SessionStats {
        SessionStats {
            session_id: "s".into(),
            turns: (0..n)
                .map(|i| Turn {
                    model: "claude-sonnet-5".into(),
                    usage: Usage::default(),
                    timestamp: Some(format!("2026-08-{:02}T00:00:00.000Z", 1 + i.min(27))),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn flags_generic_boilerplate() {
        let report = lint("t", "Project: foo\nAlways write clean code.\nUse Postgres.\n", &[]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].kind(), FindingKind::Boilerplate);
        assert_eq!(report.findings[0].line, 2);
    }

    #[test]
    fn flags_an_exact_duplicate_and_not_the_original() {
        let report = lint("t", "Use Postgres.\nSomething else.\nUse Postgres.\n", &[]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].kind(), FindingKind::Duplicate);
        assert_eq!(report.findings[0].line, 3);
        assert!(matches!(report.findings[0].reason, Reason::Duplicate { origin_line: 1 }));
    }

    #[test]
    fn only_boilerplate_and_duplicate_are_auto_fixable() {
        assert!(FindingKind::Boilerplate.auto_fixable());
        assert!(FindingKind::Duplicate.auto_fixable());
        assert!(!FindingKind::StalePath.auto_fixable());
        assert!(!FindingKind::UnusedMcpServer.auto_fixable());
    }

    #[test]
    fn apply_fixes_removes_only_flagged_lines_and_preserves_the_rest() {
        let content = "Keep this.\nAlways write clean code.\nKeep this too.\nAlways write clean code again.\n";
        let report = lint("t", content, &[]);
        let (new_content, removed) = apply_fixes(content, &report);
        assert_eq!(removed.len(), 2);
        assert_eq!(new_content, "Keep this.\nKeep this too.\n");
    }

    #[test]
    fn apply_fixes_is_a_no_op_when_nothing_is_fixable() {
        let content = "Use Postgres for storage.\n";
        let report = lint("t", content, &[]);
        let (new_content, removed) = apply_fixes(content, &report);
        assert!(removed.is_empty());
        assert_eq!(new_content, content);
    }

    #[test]
    fn skips_stale_path_and_server_checks_without_session_data() {
        // Nothing to compare against, so no verdict is safer than a guess.
        let content = "See `src/never-touched.rs` for details.\n";
        let report = lint("t", content, &[]);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn flags_a_path_no_analyzed_session_touched() {
        let mut session = session_with_turns(1);
        session.tool_call_log =
            vec![ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("C:\\repo\\src\\real.rs".into()) }];
        let content = "See `src/real.rs` — fine.\nSee `src/ghost.rs` — stale.\n";
        let report = lint("t", content, std::slice::from_ref(&session));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].kind(), FindingKind::StalePath);
        assert!(report.findings[0].text.contains("ghost.rs"));
    }

    #[test]
    fn matches_a_touched_path_across_slash_and_backslash() {
        let mut session = session_with_turns(1);
        session.tool_call_log =
            vec![ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("C:\\repo\\src\\real.rs".into()) }];
        let report = lint("t", "See `src/real.rs`.\n", std::slice::from_ref(&session));
        assert!(report.findings.is_empty());
    }

    #[test]
    fn ignores_prose_slashes_outside_backticks() {
        // "8/10 sessions" and a bare URL should never read as a stale path.
        let session = session_with_turns(1);
        let content = "Aim for 8/10 test coverage. See https://example.com/docs for style.\n";
        let report = lint("t", content, std::slice::from_ref(&session));
        assert!(report.findings.is_empty());
    }

    #[test]
    fn prices_the_file_deterministically_even_with_no_session_data() {
        let report = lint("t", &"x".repeat(4000), &[]);
        assert_eq!(report.total_tokens, 1000);
        assert!(report.cost_per_1k_requests_usd > 0.0);
        assert!(report.monthly_cost_usd.is_none(), "no sessions, no volume to price a month at");
    }

    #[test]
    fn prices_a_month_only_when_there_is_volume_to_measure() {
        let sessions = vec![session_with_turns(10)];
        let report = lint("t", &"x".repeat(4000), &sessions);
        assert!(report.monthly_cost_usd.unwrap() > 0.0);
    }

    #[test]
    fn fixable_savings_never_exceed_total_cost() {
        let sessions = vec![session_with_turns(5)];
        let content = "Keep this.\nAlways write clean code.\n";
        let report = lint("t", content, &sessions);
        assert!(report.fixable_monthly_savings_usd.unwrap() <= report.monthly_cost_usd.unwrap());
    }

    #[test]
    fn an_empty_file_produces_an_empty_report_rather_than_a_panic() {
        let report = lint("t", "", &[]);
        assert!(report.findings.is_empty());
        assert_eq!(report.total_tokens, 0);
        assert_eq!(report.cost_per_1k_requests_usd, 0.0);
    }

    #[test]
    fn word_boundary_does_not_match_inside_a_longer_word() {
        assert!(!word_boundary_contains("notional coverage", "notion"));
        assert!(word_boundary_contains("uses the notion server", "notion"));
        assert!(word_boundary_contains("notion", "notion"));
    }

    #[test]
    fn analyze_optional_treats_a_missing_baseline_as_empty() {
        let path = Path::new("this-file-does-not-exist-anywhere.md");
        let report = analyze_optional(path, &[], Lang::En).unwrap();
        assert_eq!(report.total_tokens, 0);
    }
}
