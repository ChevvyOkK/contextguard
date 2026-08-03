use std::collections::HashMap;

use owo_colors::OwoColorize;

use crate::claude_md::ClaudeMdReport;
use crate::i18n::{self, Lang};
use crate::pricing::PricingTable;
use crate::savings::SavingsReport;
use crate::session::{SessionStats, Usage};

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
        for (model, turn_usage) in &session.turns {
            session_cost += pricing.cost_usd(model, turn_usage);
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
            cwd: None,
            turns: vec![(model.to_string(), usage)],
            tool_calls: HashMap::new(),
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
