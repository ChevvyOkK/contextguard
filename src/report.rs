use std::collections::HashMap;

use owo_colors::OwoColorize;

use crate::claude_md::ClaudeMdReport;
use crate::context::{self, ContextAudit, Provenance};
use crate::i18n::{self, Lang};
use crate::lint::{Finding, LintReport, Reason};
use crate::optimize;
use crate::pricing::PricingTable;
use crate::savings::SavingsReport;
use crate::budget::BudgetCheck;
use crate::git_cost::{CostItem, GitCostReport, ItemKind};
use crate::savings_report::MonthlySavings;
use crate::session::{SessionStats, Usage};

/// How many findings to show per algorithm — enough to act on, not so many
/// the report turns into noise.
const FINDINGS_PER_ALGORITHM: usize = 3;

pub struct Aggregate {
    pub sessions: usize,
    pub usage: Usage,
    pub cost_usd: f64,
    pub tool_calls: HashMap<String, u64>,
    pub top_sessions: Vec<(String, f64)>,
}

pub fn aggregate(sessions: &[SessionStats], pricing: &PricingTable) -> Aggregate {
    let mut usage = Usage::default();
    let mut cost_usd = 0.0;
    let mut tool_calls: HashMap<String, u64> = HashMap::new();
    let mut top_sessions: Vec<(String, f64)> = Vec::new();

    for session in sessions {
        let mut session_cost = 0.0;
        for turn in &session.turns {
            session_cost += pricing.cost_usd(&turn.model, &turn.usage);
        }
        cost_usd += session_cost;
        usage.add(&session.total_usage());
        top_sessions.push((session.session_id.clone(), session_cost));

        for (name, count) in &session.tool_calls {
            *tool_calls.entry(name.clone()).or_insert(0) += count;
        }
    }

    top_sessions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Aggregate { sessions: sessions.len(), usage, cost_usd, tool_calls, top_sessions }
}

fn cache_efficiency(usage: &Usage) -> f64 {
    let total_input = usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens;
    if total_input == 0 {
        return 0.0;
    }
    usage.cache_read_input_tokens as f64 / total_input as f64 * 100.0
}

pub fn print_report(agg: &Aggregate, claude_md: Option<&ClaudeMdReport>, savings: &SavingsReport, lang: Lang) {
    println!("{}", i18n::report_title(lang).bold());
    println!();

    if savings.interventions > 0 {
        println!("{}", i18n::plugin_savings_header(lang).bold().cyan());
        println!("{}", i18n::plugin_savings_line(lang, savings.interventions, &format_num(savings.tokens_saved_estimate)));
        println!();
    }

    println!("{} {}", i18n::sessions_analyzed(lang), agg.sessions.to_string().bold());
    println!(
        "{}",
        i18n::tokens_line(
            lang,
            &format_num(agg.usage.input_tokens),
            &format_num(agg.usage.cache_creation_input_tokens),
            &format_num(agg.usage.cache_read_input_tokens),
            &format_num(agg.usage.output_tokens),
        )
    );
    println!("{} {}", i18n::cost_estimate_label(lang), format!("${:.2}", agg.cost_usd).bold().green());

    let efficiency = cache_efficiency(&agg.usage);
    let efficiency_str = format!("{efficiency:.0}%");
    if efficiency < 50.0 {
        println!(
            "{} {} — {}",
            i18n::cache_efficiency_label(lang),
            efficiency_str.yellow(),
            i18n::cache_efficiency_warning(lang)
        );
    } else {
        println!("{} {}", i18n::cache_efficiency_label(lang), efficiency_str.green());
    }

    if agg.top_sessions.len() > 1 {
        println!();
        println!("{}", i18n::top_sessions_header(lang).bold());
        for (id, cost) in agg.top_sessions.iter().take(5) {
            println!("  {}  {}", format!("${cost:.2}").bold(), id.dimmed());
        }
    }

    if !agg.tool_calls.is_empty() {
        println!();
        println!("{}", i18n::tools_header(lang).bold());
        let mut sorted: Vec<_> = agg.tool_calls.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in sorted.iter().take(10) {
            println!("  {count:>5}  {name}");
        }
    }

    if let Some(report) = claude_md {
        println!();
        println!("{}", i18n::claude_md_header(lang).bold());
        println!("{} {}", i18n::claude_md_path_label(lang), report.path);
        println!(
            "{}{}",
            i18n::claude_md_length_line(lang, report.line_count, report.approx_tokens),
            if report.over_recommended {
                format!(" {}", i18n::claude_md_over_recommended(lang, crate::claude_md::RECOMMENDED_MAX_LINES).yellow())
            } else {
                String::new()
            }
        );
        if !report.generic_lines.is_empty() {
            println!("{}", i18n::claude_md_generic_lines_intro(lang, report.generic_lines.len()));
            for (line_no, text) in report.generic_lines.iter().take(5) {
                println!("    {}: {}", i18n::claude_md_line_label(lang, *line_no).dimmed(), text);
            }
        }
    }

    println!();
    println!("{}", i18n::suggestions_header(lang).bold());
    let mut suggestions = Vec::new();
    if efficiency < 50.0 {
        suggestions.push(i18n::suggestion_cache_efficiency(lang).to_string());
    }
    if let Some(report) = claude_md {
        if report.over_recommended {
            suggestions.push(i18n::suggestion_claude_md_length(lang, crate::claude_md::RECOMMENDED_MAX_LINES));
        }
        if !report.generic_lines.is_empty() {
            suggestions.push(i18n::suggestion_claude_md_generic(lang).to_string());
        }
    }
    if suggestions.is_empty() {
        println!("  {}", i18n::no_issues_found(lang));
    } else {
        for s in &suggestions {
            println!("  {} {s}", "—".cyan());
        }
    }
}

/// Runs all six Cost-Optimization Engine detectors and prints every
/// resulting finding as exactly three lines: a dollar loss, a reason, and a
/// one-line fix. `claude_md` is the already-parsed report (if any) so this
/// doesn't re-read the file.
pub fn print_optimizations(
    sessions: &[SessionStats],
    pricing: &PricingTable,
    claude_md: Option<&ClaudeMdReport>,
    lang: Lang,
) {
    let cache_churn = optimize::detect_cache_churn(sessions, pricing);
    let re_reads = optimize::detect_re_reads(sessions, pricing);
    let claude_md_finding = claude_md.and_then(|r| optimize::detect_claude_md_amortized(r, sessions, pricing));
    let burn_rate = optimize::detect_burn_rate(sessions, pricing);
    let context_growth = optimize::detect_context_growth(sessions, pricing);
    let model_mismatch = optimize::detect_model_mismatch(sessions, pricing);

    let any_findings = !cache_churn.is_empty()
        || !re_reads.is_empty()
        || claude_md_finding.is_some()
        || !burn_rate.is_empty()
        || !context_growth.is_empty()
        || !model_mismatch.is_empty();

    println!();
    println!("{}", i18n::optimize_header(lang).bold());

    if !any_findings {
        println!("  {}", i18n::optimize_none_found(lang));
        return;
    }

    for f in cache_churn.iter().take(FINDINGS_PER_ALGORITHM) {
        print_finding(f.loss_usd, i18n::optimize_cache_churn_reason(lang, &f.session_id, f.churn_pct, f.turns), i18n::optimize_cache_churn_action(lang), lang);
    }

    for f in re_reads.iter().take(FINDINGS_PER_ALGORITHM) {
        print_finding(f.loss_usd, i18n::optimize_re_read_reason(lang, &f.path, f.read_count, &f.session_id), i18n::optimize_re_read_action(lang), lang);
    }

    if let (Some(report), Some(f)) = (claude_md, &claude_md_finding) {
        print_finding(
            f.savings_usd,
            i18n::optimize_claude_md_reason(lang, report.line_count, report.approx_tokens, f.monthly_cost_usd),
            i18n::optimize_claude_md_action(lang, f.target_lines),
            lang,
        );
    }

    for f in burn_rate.iter().take(FINDINGS_PER_ALGORITHM) {
        print_finding(f.loss_usd, i18n::optimize_burn_rate_reason(lang, &f.session_id, f.usd_per_hour, f.p95_usd_per_hour), i18n::optimize_burn_rate_action(lang), lang);
    }

    for f in context_growth.iter().take(FINDINGS_PER_ALGORITHM) {
        print_finding(
            f.loss_usd,
            i18n::optimize_context_growth_reason(lang, &f.session_id, f.growth_ratio, f.turns),
            i18n::optimize_context_growth_action(lang, f.optimal_turn),
            lang,
        );
    }

    for f in model_mismatch.iter().take(FINDINGS_PER_ALGORITHM) {
        print_finding(f.loss_usd, i18n::optimize_model_mismatch_reason(lang, &f.session_id, f.flagged_turns), i18n::optimize_model_mismatch_action(lang), lang);
    }
}

/// First 8 chars of a session id — enough to reference it in a compact
/// report without the full UUID eating the line.
fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

struct MarkdownItem {
    title: &'static str,
    loss_line: String,
    action_line: String,
    loss_usd: f64,
}

/// Compact Markdown audit report for pasting into Slack or a GitHub PR:
/// a one-line summary plus the top 3 findings *across all six detectors
/// combined* (ranked by $ loss), unlike the text report's per-algorithm
/// sections capped at 3 each.
pub fn print_markdown_report(
    agg: &Aggregate,
    claude_md: Option<&ClaudeMdReport>,
    sessions: &[SessionStats],
    pricing: &PricingTable,
    lang: Lang,
) {
    println!("{}", i18n::markdown_report_title(lang));
    println!("{}", i18n::markdown_summary_line(lang, agg.sessions, agg.cost_usd));

    let efficiency = cache_efficiency(&agg.usage);
    println!("{}", i18n::markdown_cache_hit_line(lang, efficiency, efficiency >= 85.0));
    println!();
    println!("{}", i18n::markdown_top_issues_header(lang));

    let mut items: Vec<MarkdownItem> = Vec::new();

    for f in optimize::detect_cache_churn(sessions, pricing) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_cache_churn(lang),
            loss_line: i18n::markdown_loss_cache_churn(lang, f.loss_usd, &short_id(&f.session_id)),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_cache_churn_action(lang)),
            loss_usd: f.loss_usd,
        });
    }

    for f in optimize::detect_re_reads(sessions, pricing) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_re_read(lang),
            loss_line: i18n::markdown_loss_re_read(lang, f.loss_usd, &f.path, f.read_count),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_re_read_action(lang)),
            loss_usd: f.loss_usd,
        });
    }

    let claude_md_finding = claude_md.and_then(|r| optimize::detect_claude_md_amortized(r, sessions, pricing));
    if let (Some(report), Some(f)) = (claude_md, &claude_md_finding) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_claude_md(lang),
            loss_line: i18n::markdown_loss_claude_md(lang, f.savings_usd, report.line_count),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_claude_md_action(lang, f.target_lines)),
            loss_usd: f.savings_usd,
        });
    }

    for f in optimize::detect_burn_rate(sessions, pricing) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_burn_rate(lang),
            loss_line: i18n::markdown_loss_burn_rate(lang, f.loss_usd, &short_id(&f.session_id)),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_burn_rate_action(lang)),
            loss_usd: f.loss_usd,
        });
    }

    for f in optimize::detect_context_growth(sessions, pricing) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_context_growth(lang),
            loss_line: i18n::markdown_loss_context_growth(lang, f.loss_usd, &short_id(&f.session_id)),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_context_growth_action(lang, f.optimal_turn)),
            loss_usd: f.loss_usd,
        });
    }

    for f in optimize::detect_model_mismatch(sessions, pricing) {
        items.push(MarkdownItem {
            title: i18n::markdown_title_model_mismatch(lang),
            loss_line: i18n::markdown_loss_model_mismatch(lang, f.loss_usd, &short_id(&f.session_id), f.flagged_turns),
            action_line: format!("{} {}", i18n::markdown_action_label(lang), i18n::optimize_model_mismatch_action(lang)),
            loss_usd: f.loss_usd,
        });
    }

    items.sort_by(|a, b| b.loss_usd.partial_cmp(&a.loss_usd).unwrap_or(std::cmp::Ordering::Equal));

    if items.is_empty() {
        println!("{}", i18n::markdown_no_issues(lang));
        return;
    }

    for (i, item) in items.iter().take(3).enumerate() {
        println!("{}. {}", i + 1, item.title);
        println!("   - {}", item.loss_line);
        println!("   - {}", item.action_line);
    }
}

fn print_finding(loss_usd: f64, reason: String, action: impl std::fmt::Display, lang: Lang) {
    println!();
    println!("  {}", i18n::optimize_loss_line(lang, loss_usd).yellow().bold());
    println!("  {reason}");
    println!("  {} {action}", i18n::optimize_action_prefix(lang).cyan());
}

fn format_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(' ');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Renders the context audit.
///
/// Bars are proportional to occupancy so the shape of the answer is visible
/// before any number is read — on real transcripts one or two categories
/// dwarf everything else, and that is the finding.
pub fn print_context_audit(audit: &ContextAudit, lang: Lang) {
    println!("{}", i18n::context_title(lang).bold());
    println!("{}", i18n::context_subtitle(lang, audit.sessions).dimmed());
    println!();

    if audit.categories.is_empty() {
        println!("{}", i18n::context_nothing(lang).yellow());
        return;
    }

    let widest = audit.categories.iter().map(|c| c.label.chars().count()).max().unwrap_or(0);
    let largest = audit.categories.iter().map(|c| c.tokens).max().unwrap_or(1).max(1);

    for category in &audit.categories {
        let filled = ((category.tokens as f64 / largest as f64) * 28.0).round() as usize;
        let bar = "█".repeat(filled.max(1));
        let share = 100.0 * category.tokens as f64 / audit.total_tokens.max(1) as f64;
        let marker = match category.provenance {
            Provenance::Estimated => format!(" {}", i18n::context_estimated_marker(lang)).dimmed().to_string(),
            Provenance::Measured => String::new(),
        };
        println!(
            "  {label:<width$}  {tokens:>12} tok  {share:>4.0}%  {bar}{marker}",
            label = category.label,
            width = widest,
            tokens = format_num(category.tokens),
            share = share,
            bar = bar.cyan(),
            marker = marker,
        );
    }

    println!();
    println!(
        "{}",
        i18n::context_carried_note(
            lang,
            &format_num(audit.categories.iter().map(|c| c.carried_tokens).sum()),
            &format!("${:.2}", audit.carried_cost_usd),
        )
    );

    if let Some(prefix) = audit.fixed_prefix_tokens {
        println!();
        println!("{}", i18n::context_prefix_note(lang, &format_num(prefix)).dimmed());
    }

    let configured = context::configured_servers();
    if !audit.mcp_servers.is_empty() || !configured.is_empty() {
        println!();
        println!("{}", i18n::context_mcp_header(lang).bold());
        for server in &audit.mcp_servers {
            println!("  {:<24} {} calls", server.server, server.calls);
        }
        let unused = context::unused_servers(&configured, audit);
        if !unused.is_empty() {
            println!(
                "  {}",
                i18n::context_mcp_unused(lang, unused.len(), &unused.join(", ")).yellow()
            );
        }
        println!("  {}", i18n::context_mcp_caveat(lang).dimmed());
    }

    if !audit.repeated_reads.is_empty() {
        println!();
        println!("{}", i18n::context_rereads_header(lang).bold());
        for read in &audit.repeated_reads {
            println!("  {}", i18n::context_reread_line(lang, read.count, &read.path).yellow());
        }
    }
}

/// Machine-readable form of the same audit.
///
/// Hand-built rather than derived: the structs would need serde derives that
/// nothing else in this binary wants, and the field names here are a public
/// interface that should not silently follow a Rust rename.
pub fn context_audit_json(audit: &ContextAudit) -> String {
    let categories: Vec<String> = audit
        .categories
        .iter()
        .map(|c| {
            format!(
                r#"{{"label":{},"tokens":{},"carried_tokens":{},"provenance":"{}"}}"#,
                json_string(&c.label),
                c.tokens,
                c.carried_tokens,
                match c.provenance {
                    Provenance::Measured => "measured",
                    Provenance::Estimated => "estimated",
                }
            )
        })
        .collect();

    let reads: Vec<String> = audit
        .repeated_reads
        .iter()
        .map(|r| format!(r#"{{"path":{},"count":{}}}"#, json_string(&r.path), r.count))
        .collect();

    let servers: Vec<String> = audit
        .mcp_servers
        .iter()
        .map(|s| format!(r#"{{"server":{},"calls":{}}}"#, json_string(&s.server), s.calls))
        .collect();

    format!(
        r#"{{"sessions":{},"total_tokens":{},"carried_cost_usd":{:.4},"fixed_prefix_tokens":{},"categories":[{}],"repeated_reads":[{}],"mcp_servers":[{}]}}"#,
        audit.sessions,
        audit.total_tokens,
        audit.carried_cost_usd,
        audit.fixed_prefix_tokens.map(|t| t.to_string()).unwrap_or_else(|| "null".to_string()),
        categories.join(","),
        reads.join(","),
        servers.join(","),
    )
}

// --- lint ----------------------------------------------------------------

/// Turns a structured `Reason` into a sentence in the requested language.
/// The only place lint findings get translated — src/lint.rs stores data,
/// not prose, the same split context.rs uses for `Provenance`.
fn reason_text(lang: Lang, reason: &Reason) -> String {
    match reason {
        Reason::Boilerplate { phrase } => i18n::lint_reason_boilerplate(lang, phrase),
        Reason::Duplicate { origin_line } => i18n::lint_reason_duplicate(lang, *origin_line),
        Reason::StalePath { token } => i18n::lint_reason_stale_path(lang, token),
        Reason::UnusedMcpServer { server } => i18n::lint_reason_unused_server(lang, server),
    }
}

fn print_lint_findings(findings: &[Finding], lang: Lang) {
    let kind_width = findings.iter().map(|f| i18n::lint_kind_label(lang, f.kind()).chars().count()).max().unwrap_or(0);
    for f in findings {
        let label = i18n::lint_kind_label(lang, f.kind());
        println!("  {label:<kind_width$}  L{line:<5} {text}", line = f.line, text = f.text);
        println!("  {:kind_width$}          {}", "", reason_text(lang, &f.reason).dimmed());
    }
}

fn print_lint_cost_lines(report: &LintReport, lang: Lang) {
    println!("{}", i18n::lint_cost_per_1k(lang, report.cost_per_1k_requests_usd).dimmed());
    match report.monthly_cost_usd {
        Some(monthly) => {
            println!("{}", i18n::lint_monthly_cost(lang, monthly));
            if let Some(savings) = report.fixable_monthly_savings_usd {
                if savings > 0.0 {
                    println!("{}", i18n::lint_monthly_savings(lang, savings).green());
                }
            }
        }
        None => println!("{}", i18n::lint_no_local_volume(lang).dimmed()),
    }
}

pub fn print_lint(report: &LintReport, lang: Lang) {
    println!("{}", i18n::lint_title(lang, &report.path).bold());
    println!("{}", i18n::lint_summary(lang, report.line_count, report.total_tokens).dimmed());
    println!();

    if report.findings.is_empty() {
        println!("{}", i18n::lint_clean(lang).green());
    } else {
        print_lint_findings(&report.findings, lang);
        println!();
        let fixable = report.findings.iter().filter(|f| f.kind().auto_fixable()).count();
        if fixable > 0 {
            println!("{}", i18n::lint_fixable_note(lang, fixable, report.fixable_tokens).yellow());
        }
    }

    println!();
    print_lint_cost_lines(report, lang);
}

pub fn lint_markdown(report: &LintReport, lang: Lang) -> String {
    let mut out = String::new();
    out.push_str(&format!("**{}**\n\n", i18n::lint_title(lang, &report.path)));
    out.push_str(&format!("{}\n\n", i18n::lint_summary(lang, report.line_count, report.total_tokens)));

    if report.findings.is_empty() {
        out.push_str(&format!("{}\n\n", i18n::lint_clean(lang)));
    } else {
        for f in &report.findings {
            out.push_str(&format!(
                "- **{}** (line {}): `{}`\n  {}\n",
                i18n::lint_kind_label(lang, f.kind()),
                f.line,
                f.text,
                reason_text(lang, &f.reason)
            ));
        }
        out.push('\n');
        let fixable = report.findings.iter().filter(|f| f.kind().auto_fixable()).count();
        if fixable > 0 {
            out.push_str(&format!("{}\n\n", i18n::lint_fixable_note(lang, fixable, report.fixable_tokens)));
        }
    }

    out.push_str(&format!("{}\n", i18n::lint_cost_per_1k(lang, report.cost_per_1k_requests_usd)));
    match report.monthly_cost_usd {
        Some(monthly) => out.push_str(&format!("{}\n", i18n::lint_monthly_cost(lang, monthly))),
        None => out.push_str(&format!("_{}_\n", i18n::lint_no_local_volume(lang))),
    }
    out
}

pub fn print_lint_fix_preview(removed: &[Finding], lang: Lang) {
    if removed.is_empty() {
        println!("{}", i18n::lint_fix_nothing(lang).dimmed());
        return;
    }
    println!();
    println!("{}", i18n::lint_fix_preview_header(lang, removed.len()).bold());
    for f in removed {
        println!("  {} L{:<5} {}", "-".red(), f.line, f.text);
        println!("    {}", reason_text(lang, &f.reason).dimmed());
    }
}

pub fn print_lint_fix_done(removed: usize, remaining_lines: usize, lang: Lang) {
    println!();
    println!("{}", i18n::lint_fix_done(lang, removed, remaining_lines).green());
}

/// Token delta between two lint reports of the same file at different
/// revisions. Positive means the change added tokens.
fn token_delta(baseline: &LintReport, current: &LintReport) -> i64 {
    current.total_tokens as i64 - baseline.total_tokens as i64
}

pub fn print_lint_compare(baseline: &LintReport, current: &LintReport, lang: Lang) {
    println!("{}", i18n::lint_compare_title(lang, &baseline.path, &current.path).bold());

    let delta = token_delta(baseline, current);
    if delta == 0 {
        println!("{}", i18n::lint_compare_no_change(lang));
        return;
    }
    println!("{}", i18n::lint_compare_delta(lang, delta, baseline.total_tokens, current.total_tokens));

    let per_1k = (delta.unsigned_abs() as f64 / 1_000_000.0)
        * PricingTable::defaults().for_model("claude-sonnet-5").cache_read_per_mtok
        * 1000.0;
    println!(
        "{}",
        if delta > 0 { i18n::lint_compare_price_added(lang, per_1k) } else { i18n::lint_compare_price_saved(lang, per_1k) }
    );

    if let (Some(before), Some(after)) = (baseline.monthly_cost_usd, current.monthly_cost_usd) {
        println!("{}", i18n::lint_compare_monthly(lang, (after - before).abs()));
    }
}

pub fn lint_compare_markdown(baseline: &LintReport, current: &LintReport, lang: Lang) -> String {
    let mut out = String::new();
    out.push_str(&format!("**{}**\n\n", i18n::lint_compare_title(lang, &baseline.path, &current.path)));

    let delta = token_delta(baseline, current);
    if delta == 0 {
        out.push_str(&format!("{}\n", i18n::lint_compare_no_change(lang)));
        return out;
    }
    out.push_str(&format!("{}\n\n", i18n::lint_compare_delta(lang, delta, baseline.total_tokens, current.total_tokens)));

    let per_1k = (delta.unsigned_abs() as f64 / 1_000_000.0)
        * PricingTable::defaults().for_model("claude-sonnet-5").cache_read_per_mtok
        * 1000.0;
    out.push_str(&format!(
        "{}\n",
        if delta > 0 { i18n::lint_compare_price_added(lang, per_1k) } else { i18n::lint_compare_price_saved(lang, per_1k) }
    ));

    if let (Some(before), Some(after)) = (baseline.monthly_cost_usd, current.monthly_cost_usd) {
        out.push_str(&format!("{}\n", i18n::lint_compare_monthly(lang, (after - before).abs())));
    }

    out.push_str(&format!("\n_{}_\n", i18n::lint_compare_footer(lang)));
    out
}

/// Minimal JSON string escaping. Paths on Windows are full of backslashes,
/// which would produce invalid JSON if emitted raw.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// --- savings report --------------------------------------------------------

fn print_month_savings(month: &MonthlySavings, lang: Lang) {
    let label = i18n::savings_month_label(lang, &month.month);
    println!("{}", i18n::savings_title(lang, &label).bold());

    if month.total_bash_interventions == 0 {
        println!("{}", i18n::savings_nothing_this_month(lang).dimmed());
    } else {
        println!(
            "{} {}",
            i18n::savings_headline(lang, month.bash_truncate_tokens, month.bash_truncate_usd).green(),
            i18n::savings_amortized_note(lang).dimmed()
        );
        println!("{}", i18n::savings_from_line(lang, month.total_bash_interventions, month.sessions_touched));

        if month.amortized_interventions == 0 {
            println!("{}", i18n::savings_no_amortization_note(lang).dimmed());
        } else if month.amortized_interventions < month.total_bash_interventions {
            println!(
                "{}",
                i18n::savings_partial_amortization_note(lang, month.amortized_interventions, month.total_bash_interventions)
                    .dimmed()
            );
        }

        if let Some(top) = &month.top_command {
            println!("{}", i18n::savings_top_command(lang, &top.label, top.tokens));
        }
    }

    if month.grep_cap_interventions > 0 {
        println!("{}", i18n::savings_grep_cap_line(lang, month.grep_cap_interventions).dimmed());
    }
}

pub fn print_savings_report(months: &[MonthlySavings], lang: Lang) {
    if months.is_empty() {
        println!("{}", i18n::savings_empty(lang).yellow());
        return;
    }
    for (i, month) in months.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print_month_savings(month, lang);
    }
}

fn month_savings_markdown(month: &MonthlySavings, lang: Lang) -> String {
    let mut out = String::new();
    let label = i18n::savings_month_label(lang, &month.month);
    out.push_str(&format!("**{}**\n\n", i18n::savings_title(lang, &label)));

    if month.total_bash_interventions == 0 {
        out.push_str(&format!("{}\n", i18n::savings_nothing_this_month(lang)));
    } else {
        out.push_str(&format!(
            "{} {}\n",
            i18n::savings_headline(lang, month.bash_truncate_tokens, month.bash_truncate_usd),
            i18n::savings_amortized_note(lang)
        ));
        out.push_str(&format!(
            "{}\n",
            i18n::savings_from_line(lang, month.total_bash_interventions, month.sessions_touched)
        ));

        if month.amortized_interventions == 0 {
            out.push_str(&format!("{}\n", i18n::savings_no_amortization_note(lang)));
        } else if month.amortized_interventions < month.total_bash_interventions {
            out.push_str(&format!(
                "{}\n",
                i18n::savings_partial_amortization_note(lang, month.amortized_interventions, month.total_bash_interventions)
            ));
        }

        if let Some(top) = &month.top_command {
            out.push_str(&format!("{}\n", i18n::savings_top_command(lang, &top.label, top.tokens)));
        }
    }

    if month.grep_cap_interventions > 0 {
        out.push_str(&format!("{}\n", i18n::savings_grep_cap_line(lang, month.grep_cap_interventions)));
    }

    out
}

pub fn savings_report_markdown(months: &[MonthlySavings], lang: Lang) -> String {
    if months.is_empty() {
        return i18n::savings_empty(lang).to_string();
    }
    months.iter().map(|m| month_savings_markdown(m, lang)).collect::<Vec<_>>().join("\n")
}

// --- budget --------------------------------------------------------------

pub fn print_budget_check(check: &BudgetCheck, lang: Lang) {
    let line = i18n::budget_status_line(lang, &check.period_key, check.spend_usd, check.max_usd);
    if check.crossed() {
        println!("{}", line.red().bold());
        println!("{}", i18n::budget_crossed_note(lang).dimmed());
    } else {
        println!("{}", line.green());
    }
}

// --- git-cost --------------------------------------------------------------

fn format_duration_secs(secs: i64) -> String {
    if secs <= 0 {
        return "0m".to_string();
    }
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn git_cost_item_duration(item: &CostItem) -> String {
    if item.squashed {
        "—".to_string()
    } else {
        format_duration_secs(item.last_commit_epoch - item.first_commit_epoch)
    }
}

pub fn print_git_cost(report: &GitCostReport, lang: Lang) {
    println!("{}", i18n::git_cost_title(lang).bold());
    println!(
        "{}",
        i18n::git_cost_subtitle(lang, &report.base_branch, report.lookback_secs as f64 / 3600.0).dimmed()
    );
    println!();

    if report.items.is_empty() {
        println!("{}", i18n::git_cost_empty(lang).yellow());
        return;
    }

    let widest = report.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0);

    for item in &report.items {
        let duration = git_cost_item_duration(item);
        let meta = i18n::git_cost_item_meta(lang, item.commit_count, item.turns_counted, &item.first_commit_date_display);
        let cost_str = format!("${:.2}", item.cost_usd);
        let cost_colored = match item.kind {
            ItemKind::Branch => cost_str.cyan().to_string(),
            ItemKind::MergedPr => cost_str.magenta().to_string(),
        };
        let squash_note = if item.squashed {
            format!("  ({})", i18n::git_cost_squash_marker(lang))
        } else {
            String::new()
        };
        println!(
            "  {label:<width$}  {cost:>10}   {duration:>7}   {meta}{squash_note}",
            label = item.label,
            width = widest,
            cost = cost_colored,
            duration = duration,
            meta = meta.dimmed(),
        );
    }

    println!();
    if report.repo_turns_found == 0 {
        println!("{}", i18n::git_cost_no_local_sessions(lang).yellow());
    } else {
        println!("{}", i18n::git_cost_footer(lang, report.repo_turns_found).dimmed());
    }
}

pub fn git_cost_markdown(report: &GitCostReport, lang: Lang) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "**{}** ({})\n\n",
        i18n::git_cost_title(lang),
        i18n::git_cost_subtitle(lang, &report.base_branch, report.lookback_secs as f64 / 3600.0)
    ));

    if report.items.is_empty() {
        out.push_str(i18n::git_cost_empty(lang));
        return out;
    }

    out.push_str("| | Cost | Duration | Commits | Turns matched | Since |\n|---|---|---|---|---|---|\n");
    for item in &report.items {
        let duration = git_cost_item_duration(item);
        let squash_note = if item.squashed { format!(" ({})", i18n::git_cost_squash_marker(lang)) } else { String::new() };
        out.push_str(&format!(
            "| {} | ${:.2} | {} | {}{} | {} | {} |\n",
            item.label, item.cost_usd, duration, item.commit_count, squash_note, item.turns_counted, item.first_commit_date_display
        ));
    }

    out.push('\n');
    out.push_str(&i18n::git_cost_footer(lang, report.repo_turns_found));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with(model: &str, usage: Usage) -> SessionStats {
        SessionStats {
            session_id: "s".into(),
            turns: vec![crate::session::Turn { model: model.to_string(), usage, timestamp: None }],
            ..Default::default()
        }
    }

    #[test]
    fn aggregate_sums_usage_across_sessions() {
        let pricing = PricingTable::defaults();
        let sessions = vec![
            session_with(
                "claude-sonnet-5",
                Usage { input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
            ),
            session_with(
                "claude-sonnet-5",
                Usage { input_tokens: 200, output_tokens: 100, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
            ),
        ];
        let agg = aggregate(&sessions, &pricing);
        assert_eq!(agg.sessions, 2);
        assert_eq!(agg.usage.input_tokens, 300);
        assert_eq!(agg.usage.output_tokens, 150);
        assert!(agg.cost_usd > 0.0);
    }

    #[test]
    fn cache_efficiency_is_zero_with_no_cache_usage() {
        let usage = Usage { input_tokens: 100, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
        assert_eq!(cache_efficiency(&usage), 0.0);
    }

    #[test]
    fn cache_efficiency_reflects_high_reuse() {
        let usage = Usage { input_tokens: 10, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 90 };
        let eff = cache_efficiency(&usage);
        assert!(eff > 85.0 && eff <= 100.0);
    }

    #[test]
    fn format_num_adds_thousands_separators() {
        assert_eq!(format_num(1234567), "1 234 567");
        assert_eq!(format_num(42), "42");
    }
}
