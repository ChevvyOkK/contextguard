use std::collections::HashMap;

use owo_colors::OwoColorize;

use crate::claude_md::ClaudeMdReport;
use crate::i18n::{self, Lang};
use crate::optimize;
use crate::pricing::PricingTable;
use crate::savings::SavingsReport;
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
