//! Cost-Optimization Engine: six independent detectors that turn the raw
//! per-turn usage data (session.rs) into concrete "you're losing $X here"
//! findings. Every finding is deliberately exactly three things — a dollar
//! loss, a reason, and a one-line fix — formatted by report.rs via i18n.rs;
//! this module only computes the numbers.

use crate::pricing::{ModelPricing, PricingTable};
use crate::session::{SessionStats, Turn};
use crate::timeutil;

const MTOK: f64 = 1_000_000.0;

// ---------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------

/// Per-token price actually paid across `turns`, blended by each turn's
/// share of the relevant token category. Falls back to the pricing table's
/// sonnet default when there's nothing to weight by (e.g. zero input
/// tokens) — same fallback behavior as `PricingTable::for_model`.
fn blended_price(turns: &[Turn], pricing: &PricingTable, weight_of: impl Fn(&Turn) -> u64) -> ModelPricing {
    let total_weight: u64 = turns.iter().map(&weight_of).sum();
    if total_weight == 0 {
        return pricing.for_model("claude-sonnet-5");
    }
    let mut blended = ModelPricing { input_per_mtok: 0.0, output_per_mtok: 0.0, cache_write_per_mtok: 0.0, cache_read_per_mtok: 0.0 };
    for turn in turns {
        let w = weight_of(turn) as f64 / total_weight as f64;
        let p = pricing.for_model(&turn.model);
        blended.input_per_mtok += p.input_per_mtok * w;
        blended.output_per_mtok += p.output_per_mtok * w;
        blended.cache_write_per_mtok += p.cache_write_per_mtok * w;
        blended.cache_read_per_mtok += p.cache_read_per_mtok * w;
    }
    blended
}

/// Nearest-rank percentile over a copy of `values` (0.0..=1.0 for `p`).
/// Returns `None` for an empty slice.
fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

fn session_cost(session: &SessionStats, pricing: &PricingTable) -> f64 {
    session.turns.iter().map(|t| pricing.cost_usd(&t.model, &t.usage)).sum()
}

// ---------------------------------------------------------------------
// 1. Cache Churn Detector
// ---------------------------------------------------------------------

pub struct CacheChurnFinding {
    pub session_id: String,
    pub turns: usize,
    pub churn_pct: f64,
    pub loss_usd: f64,
}

/// Flags a session where turns 2+ are paying the (expensive) cache-write
/// price more often than the (cheap) cache-read price — the cache is being
/// rebuilt instead of reused. Turn 1 is excluded: every session pays a
/// cache-write on its very first turn since there's no cache yet, and
/// that's unavoidable overhead, not churn.
pub fn detect_cache_churn(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<CacheChurnFinding> {
    let mut findings = Vec::new();

    for session in sessions {
        if session.turns.len() <= 5 {
            continue;
        }
        let rest = &session.turns[1..];
        let write_tokens: u64 = rest.iter().map(|t| t.usage.cache_creation_input_tokens).sum();
        let read_tokens: u64 = rest.iter().map(|t| t.usage.cache_read_input_tokens).sum();
        let denom = write_tokens + read_tokens;
        if denom == 0 {
            continue;
        }
        let churn = write_tokens as f64 / denom as f64;
        if churn <= 0.25 {
            continue;
        }

        let loss_usd: f64 = rest
            .iter()
            .map(|t| {
                let p = pricing.for_model(&t.model);
                t.usage.cache_creation_input_tokens as f64 / MTOK * (p.cache_write_per_mtok - p.cache_read_per_mtok)
            })
            .sum();

        findings.push(CacheChurnFinding {
            session_id: session.session_id.clone(),
            turns: session.turns.len(),
            churn_pct: churn * 100.0,
            loss_usd,
        });
    }

    findings.sort_by(|a, b| b.loss_usd.partial_cmp(&a.loss_usd).unwrap_or(std::cmp::Ordering::Equal));
    findings
}

// ---------------------------------------------------------------------
// 2. Re-Read Detector
// ---------------------------------------------------------------------

pub struct ReReadFinding {
    pub session_id: String,
    pub path: String,
    pub read_count: u64,
    pub loss_usd: f64,
}

/// Flags a file read 3+ times within the same session. There's no per-tool-
/// call token cost in the transcript (the cost is baked into the
/// surrounding turn's input tokens, not itemized per tool), so the
/// overpayment is estimated: each extra read is priced at the session's own
/// average fresh-context size per turn, at its own blended input rate —
/// a real, session-specific number, just not an exact one.
pub fn detect_re_reads(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<ReReadFinding> {
    let mut findings = Vec::new();

    for session in sessions {
        if session.turns.is_empty() {
            continue;
        }
        let mut by_path: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
        for call in &session.tool_call_log {
            if call.name != "Read" {
                continue;
            }
            if let Some(path) = &call.file_path {
                *by_path.entry(path.as_str()).or_insert(0) += 1;
            }
        }

        let avg_fresh_context: f64 = session
            .turns
            .iter()
            .map(|t| (t.usage.input_tokens + t.usage.cache_creation_input_tokens) as f64)
            .sum::<f64>()
            / session.turns.len() as f64;
        let blended = blended_price(&session.turns, pricing, |t| t.usage.input_tokens);

        let mut paths: Vec<_> = by_path.into_iter().filter(|(_, count)| *count >= 3).collect();
        paths.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        for (path, count) in paths {
            let extra_reads = (count - 1) as f64;
            let loss_usd = extra_reads * avg_fresh_context / MTOK * blended.input_per_mtok;
            findings.push(ReReadFinding {
                session_id: session.session_id.clone(),
                path: path.to_string(),
                read_count: count,
                loss_usd,
            });
        }
    }

    findings.sort_by(|a, b| b.loss_usd.partial_cmp(&a.loss_usd).unwrap_or(std::cmp::Ordering::Equal));
    findings
}

// ---------------------------------------------------------------------
// 3. CLAUDE.md Amortizer
// ---------------------------------------------------------------------

pub struct ClaudeMdFinding {
    pub monthly_cost_usd: f64,
    pub savings_usd: f64,
    pub target_lines: usize,
}

/// Projects CLAUDE.md's current line count against the observed pace of
/// assistant turns (across every analyzed session, scaled to a 30-day
/// month) to estimate its ongoing monthly cost, assuming steady-state
/// cache-read reuse (the best case — if it's *not* well cached the real
/// cost is higher, not lower). Only fires when the file is already flagged
/// as over the recommended length; a short file has nothing to trim.
pub fn detect_claude_md_amortized(
    claude_md: &crate::claude_md::ClaudeMdReport,
    sessions: &[SessionStats],
    pricing: &PricingTable,
) -> Option<ClaudeMdFinding> {
    if !claude_md.over_recommended {
        return None;
    }

    let all_turns: Vec<&Turn> = sessions.iter().flat_map(|s| &s.turns).collect();
    let total_turns = all_turns.len();
    if total_turns == 0 {
        return None;
    }

    let timestamps: Vec<f64> = all_turns.iter().filter_map(|t| t.timestamp.as_deref()).filter_map(timeutil::parse_epoch_seconds).collect();
    let observed_days = if timestamps.len() >= 2 {
        let min = timestamps.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = timestamps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        ((max - min) / 86_400.0).max(1.0)
    } else {
        1.0
    };

    let monthly_turns = total_turns as f64 / observed_days * 30.0;

    // Owned copy so blended_price's `&[Turn]` signature works uniformly for
    // both the flattened cross-session slice here and per-session slices
    // elsewhere.
    let owned_turns: Vec<Turn> = all_turns.into_iter().cloned().collect();
    let blended = blended_price(&owned_turns, pricing, |t| t.usage.cache_read_input_tokens);

    let monthly_cost_usd = claude_md.approx_tokens as f64 / MTOK * blended.cache_read_per_mtok * monthly_turns;
    let target_lines = crate::claude_md::RECOMMENDED_MAX_LINES;
    let target_ratio = target_lines as f64 / claude_md.line_count as f64;
    let savings_usd = monthly_cost_usd * (1.0 - target_ratio);

    Some(ClaudeMdFinding { monthly_cost_usd, savings_usd, target_lines })
}

// ---------------------------------------------------------------------
// 4. Burn-Rate Watch
// ---------------------------------------------------------------------

pub struct BurnRateFinding {
    pub session_id: String,
    pub usd_per_hour: f64,
    pub p95_usd_per_hour: f64,
    pub loss_usd: f64,
}

/// Not a literal live daemon — this is a batch report, not a running
/// process attached to an active session — but the same idea applied
/// retrospectively: compute every session's $/hour, take your own p95 as
/// the baseline, and flag whichever session(s) blew past it. Needs at
/// least 2 timestamped turns per session (for a duration) and at least 3
/// qualifying sessions overall (for a percentile that means anything).
pub fn detect_burn_rate(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<BurnRateFinding> {
    let mut rates: Vec<(String, f64, f64)> = Vec::new(); // (id, usd_per_hour, duration_hours)

    for session in sessions {
        let timestamps: Vec<f64> = session.turns.iter().filter_map(|t| t.timestamp.as_deref()).filter_map(timeutil::parse_epoch_seconds).collect();
        if timestamps.len() < 2 {
            continue;
        }
        let min = timestamps.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = timestamps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let duration_hours = (max - min) / 3600.0;
        if duration_hours <= 0.0 {
            continue;
        }
        let cost = session_cost(session, pricing);
        rates.push((session.session_id.clone(), cost / duration_hours, duration_hours));
    }

    if rates.len() < 3 {
        return Vec::new();
    }

    let all_rates: Vec<f64> = rates.iter().map(|(_, r, _)| *r).collect();
    let Some(p95) = percentile(&all_rates, 0.95) else { return Vec::new() };

    let mut findings: Vec<BurnRateFinding> = rates
        .into_iter()
        .filter(|(_, rate, _)| *rate > p95)
        .map(|(id, rate, duration_hours)| BurnRateFinding {
            session_id: id,
            usd_per_hour: rate,
            p95_usd_per_hour: p95,
            loss_usd: (rate - p95) * duration_hours,
        })
        .collect();

    findings.sort_by(|a, b| b.usd_per_hour.partial_cmp(&a.usd_per_hour).unwrap_or(std::cmp::Ordering::Equal));
    findings
}

// ---------------------------------------------------------------------
// 5. Context Growth Advisor
// ---------------------------------------------------------------------

pub struct ContextGrowthFinding {
    pub session_id: String,
    pub turns: usize,
    pub optimal_turn: usize, // 1-indexed, for display
    pub growth_ratio: f64,
    pub loss_usd: f64,
}

/// Uses cache_read_input_tokens per turn as the context-size signal — how
/// much accumulated conversation gets replayed into each turn. A session
/// that's ever been `/compact`-ed shows a sharp drop somewhere in that
/// sequence; one that hasn't just climbs. Needs at least 6 turns for the
/// trend to mean anything.
pub fn detect_context_growth(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<ContextGrowthFinding> {
    let mut findings = Vec::new();

    for session in sessions {
        let n = session.turns.len();
        if n < 6 {
            continue;
        }
        let signal: Vec<f64> = session.turns.iter().map(|t| t.usage.cache_read_input_tokens as f64).collect();

        // Any halving-or-more drop between consecutive turns is treated as
        // a compaction event (or a fresh sub-session) — nothing to advise.
        let had_compaction = signal.windows(2).any(|w| w[0] > 1000.0 && w[1] < w[0] * 0.5);
        if had_compaction {
            continue;
        }

        let quarter = (n / 4).max(1);
        let baseline = signal[..quarter].iter().sum::<f64>() / quarter as f64;
        let tail = signal[n - quarter..].iter().sum::<f64>() / quarter as f64;
        if baseline <= 0.0 || tail < baseline * 1.8 {
            continue;
        }

        let optimal_index = signal.iter().position(|&v| v > baseline * 2.0).unwrap_or(n / 2);
        let read_price = blended_price(&session.turns, pricing, |t| t.usage.cache_read_input_tokens).cache_read_per_mtok;

        let loss_usd: f64 = signal[optimal_index..]
            .iter()
            .map(|&v| ((v - baseline).max(0.0)) / MTOK * read_price)
            .sum();

        findings.push(ContextGrowthFinding {
            session_id: session.session_id.clone(),
            turns: n,
            optimal_turn: optimal_index + 1,
            growth_ratio: tail / baseline,
            loss_usd,
        });
    }

    findings.sort_by(|a, b| b.loss_usd.partial_cmp(&a.loss_usd).unwrap_or(std::cmp::Ordering::Equal));
    findings
}

// ---------------------------------------------------------------------
// 6. Model-Mismatch
// ---------------------------------------------------------------------

pub struct ModelMismatchFinding {
    pub session_id: String,
    pub flagged_turns: usize,
    pub loss_usd: f64,
}

/// A "simple edit" turn: Opus was the model, the reply was short, and the
/// only tool calls in that turn were Edit/Write — no Task/Agent/Bash
/// orchestration alongside it that might justify the heavier model.
const SIMPLE_OUTPUT_TOKEN_CEILING: u64 = 400;
const SIMPLE_EDIT_TOOLS: &[&str] = &["Edit", "Write"];

pub fn detect_model_mismatch(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<ModelMismatchFinding> {
    let mut findings = Vec::new();
    let sonnet = pricing.for_model("claude-sonnet-5");

    for session in sessions {
        let mut tools_by_turn: std::collections::HashMap<usize, Vec<&str>> = std::collections::HashMap::new();
        for call in &session.tool_call_log {
            tools_by_turn.entry(call.turn_index).or_default().push(call.name.as_str());
        }

        let mut flagged_turns = 0usize;
        let mut loss_usd = 0.0;

        for (i, turn) in session.turns.iter().enumerate() {
            if !turn.model.to_ascii_lowercase().contains("opus") {
                continue;
            }
            if turn.usage.output_tokens >= SIMPLE_OUTPUT_TOKEN_CEILING {
                continue;
            }
            let Some(tools) = tools_by_turn.get(&i) else { continue };
            if tools.is_empty() || !tools.iter().all(|t| SIMPLE_EDIT_TOOLS.contains(t)) {
                continue;
            }

            let opus_cost = pricing.cost_usd(&turn.model, &turn.usage);
            let sonnet_cost = sonnet.input_per_mtok * turn.usage.input_tokens as f64 / MTOK
                + sonnet.output_per_mtok * turn.usage.output_tokens as f64 / MTOK
                + sonnet.cache_write_per_mtok * turn.usage.cache_creation_input_tokens as f64 / MTOK
                + sonnet.cache_read_per_mtok * turn.usage.cache_read_input_tokens as f64 / MTOK;

            flagged_turns += 1;
            loss_usd += (opus_cost - sonnet_cost).max(0.0);
        }

        if flagged_turns > 0 {
            findings.push(ModelMismatchFinding { session_id: session.session_id.clone(), flagged_turns, loss_usd });
        }
    }

    findings.sort_by(|a, b| b.loss_usd.partial_cmp(&a.loss_usd).unwrap_or(std::cmp::Ordering::Equal));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ToolCall, Usage};

    fn turn(model: &str, input: u64, output: u64, cache_write: u64, cache_read: u64, ts: Option<&str>) -> Turn {
        Turn {
            model: model.to_string(),
            usage: Usage { input_tokens: input, output_tokens: output, cache_creation_input_tokens: cache_write, cache_read_input_tokens: cache_read },
            timestamp: ts.map(str::to_string),
        }
    }

    // --- Cache Churn ---

    #[test]
    fn cache_churn_fires_when_rewrite_dominates_after_turn_one() {
        let pricing = PricingTable::defaults();
        let mut turns = vec![turn("claude-sonnet-5", 100, 50, 5000, 0, None)]; // turn 1: unavoidable write
        for _ in 0..6 {
            // Every later turn rewrites the cache instead of reading it.
            turns.push(turn("claude-sonnet-5", 100, 50, 3000, 200, None));
        }
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        let findings = detect_cache_churn(&[session], &pricing);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].churn_pct > 25.0);
        assert!(findings[0].loss_usd > 0.0);
    }

    #[test]
    fn cache_churn_does_not_fire_for_healthy_reuse() {
        let pricing = PricingTable::defaults();
        let mut turns = vec![turn("claude-sonnet-5", 100, 50, 5000, 0, None)];
        for _ in 0..6 {
            // Mostly reading from cache, barely any rewriting.
            turns.push(turn("claude-sonnet-5", 100, 50, 50, 5000, None));
        }
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        assert!(detect_cache_churn(&[session], &pricing).is_empty());
    }

    #[test]
    fn cache_churn_ignores_short_sessions() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 100, 50, 3000, 100, None); 3];
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        assert!(detect_cache_churn(&[session], &pricing).is_empty());
    }

    // --- Re-Read ---

    #[test]
    fn re_read_fires_for_a_file_read_three_times() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 2000, 100, 0, 0, None); 4];
        let tool_call_log = vec![
            ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("/a/b.rs".into()) },
            ToolCall { turn_index: 1, name: "Read".into(), file_path: Some("/a/b.rs".into()) },
            ToolCall { turn_index: 2, name: "Read".into(), file_path: Some("/a/b.rs".into()) },
        ];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        let findings = detect_re_reads(&[session], &pricing);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, "/a/b.rs");
        assert_eq!(findings[0].read_count, 3);
        assert!(findings[0].loss_usd > 0.0);
    }

    #[test]
    fn re_read_does_not_fire_for_two_reads() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 2000, 100, 0, 0, None); 2];
        let tool_call_log = vec![
            ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("/a/b.rs".into()) },
            ToolCall { turn_index: 1, name: "Read".into(), file_path: Some("/a/b.rs".into()) },
        ];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        assert!(detect_re_reads(&[session], &pricing).is_empty());
    }

    #[test]
    fn re_read_ignores_different_paths() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 2000, 100, 0, 0, None); 3];
        let tool_call_log = vec![
            ToolCall { turn_index: 0, name: "Read".into(), file_path: Some("/a.rs".into()) },
            ToolCall { turn_index: 1, name: "Read".into(), file_path: Some("/b.rs".into()) },
            ToolCall { turn_index: 2, name: "Read".into(), file_path: Some("/c.rs".into()) },
        ];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        assert!(detect_re_reads(&[session], &pricing).is_empty());
    }

    // --- CLAUDE.md Amortizer ---

    fn claude_md_report(line_count: usize, approx_tokens: usize) -> crate::claude_md::ClaudeMdReport {
        crate::claude_md::ClaudeMdReport {
            path: "CLAUDE.md".into(),
            line_count,
            approx_tokens,
            generic_lines: Vec::new(),
            over_recommended: line_count > crate::claude_md::RECOMMENDED_MAX_LINES,
        }
    }

    #[test]
    fn claude_md_amortizer_fires_for_an_oversized_file() {
        let pricing = PricingTable::defaults();
        let turns = vec![
            turn("claude-sonnet-5", 100, 50, 0, 1000, Some("2026-08-01T10:00:00.000Z")),
            turn("claude-sonnet-5", 100, 50, 0, 1000, Some("2026-08-02T10:00:00.000Z")),
        ];
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        let report = claude_md_report(400, 3800);
        let finding = detect_claude_md_amortized(&report, &[session], &pricing).unwrap();
        assert!(finding.monthly_cost_usd > 0.0);
        assert!(finding.savings_usd > 0.0);
        assert!(finding.savings_usd < finding.monthly_cost_usd);
        assert_eq!(finding.target_lines, crate::claude_md::RECOMMENDED_MAX_LINES);
    }

    #[test]
    fn claude_md_amortizer_does_not_fire_for_a_short_file() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 100, 50, 0, 1000, Some("2026-08-01T10:00:00.000Z"))];
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        let report = claude_md_report(50, 200);
        assert!(detect_claude_md_amortized(&report, &[session], &pricing).is_none());
    }

    // --- Burn-Rate Watch ---

    #[test]
    fn burn_rate_flags_the_session_that_blows_past_p95() {
        // Nearest-rank p95 needs a realistic sample size to mean anything —
        // with only a handful of sessions the top rank IS the p95 value, so
        // nothing can ever exceed it. 20 calm sessions + 1 spike is enough
        // for the spike to land above the 95th percentile of the calm ones.
        let pricing = PricingTable::defaults();
        let calm = |id: String| SessionStats {
            session_id: id,
            turns: vec![
                turn("claude-sonnet-5", 1000, 500, 0, 0, Some("2026-08-01T10:00:00.000Z")),
                turn("claude-sonnet-5", 1000, 500, 0, 0, Some("2026-08-01T11:00:00.000Z")),
            ],
            ..Default::default()
        };
        let spike = SessionStats {
            session_id: "spike".into(),
            turns: vec![
                turn("claude-opus-5", 50_000, 20_000, 0, 0, Some("2026-08-01T10:00:00.000Z")),
                turn("claude-opus-5", 50_000, 20_000, 0, 0, Some("2026-08-01T10:06:00.000Z")),
            ],
            ..Default::default()
        };
        let mut sessions: Vec<SessionStats> = (0..20).map(|i| calm(format!("calm-{i}"))).collect();
        sessions.push(spike);
        let findings = detect_burn_rate(&sessions, &pricing);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].session_id, "spike");
        assert!(findings[0].usd_per_hour > findings[0].p95_usd_per_hour);
    }

    #[test]
    fn burn_rate_needs_at_least_three_qualifying_sessions() {
        let pricing = PricingTable::defaults();
        let s = SessionStats {
            session_id: "only".into(),
            turns: vec![
                turn("claude-sonnet-5", 1000, 500, 0, 0, Some("2026-08-01T10:00:00.000Z")),
                turn("claude-sonnet-5", 1000, 500, 0, 0, Some("2026-08-01T11:00:00.000Z")),
            ],
            ..Default::default()
        };
        assert!(detect_burn_rate(&[s], &pricing).is_empty());
    }

    // --- Context Growth Advisor ---

    #[test]
    fn context_growth_fires_for_unbounded_climb_without_compaction() {
        let pricing = PricingTable::defaults();
        let turns: Vec<Turn> = (0..10).map(|i| turn("claude-sonnet-5", 200, 100, 500, 1000 * (i + 1), None)).collect();
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        let findings = detect_context_growth(&[session], &pricing);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].growth_ratio > 1.8);
        assert!(findings[0].loss_usd > 0.0);
        assert!(findings[0].optimal_turn >= 1 && findings[0].optimal_turn <= 10);
    }

    #[test]
    fn context_growth_does_not_fire_after_a_compaction_drop() {
        let pricing = PricingTable::defaults();
        let mut turns: Vec<Turn> = (0..5).map(|i| turn("claude-sonnet-5", 200, 100, 500, 5000 * (i + 1), None)).collect();
        // A sharp drop midway = a /compact happened.
        turns.push(turn("claude-sonnet-5", 200, 100, 500, 1000, None));
        turns.extend((0..4).map(|i| turn("claude-sonnet-5", 200, 100, 500, 1500 * (i + 1), None)));
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        assert!(detect_context_growth(&[session], &pricing).is_empty());
    }

    #[test]
    fn context_growth_does_not_fire_for_flat_usage() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 200, 100, 500, 3000, None); 8];
        let session = SessionStats { session_id: "s1".into(), turns, ..Default::default() };
        assert!(detect_context_growth(&[session], &pricing).is_empty());
    }

    // --- Model-Mismatch ---

    #[test]
    fn model_mismatch_flags_a_simple_opus_edit() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-opus-5", 3000, 80, 0, 0, None)];
        let tool_call_log = vec![ToolCall { turn_index: 0, name: "Edit".into(), file_path: Some("/a.rs".into()) }];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        let findings = detect_model_mismatch(&[session], &pricing);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].flagged_turns, 1);
        assert!(findings[0].loss_usd > 0.0);
    }

    #[test]
    fn model_mismatch_ignores_opus_turns_with_long_output() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-opus-5", 3000, 2000, 0, 0, None)];
        let tool_call_log = vec![ToolCall { turn_index: 0, name: "Edit".into(), file_path: Some("/a.rs".into()) }];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        assert!(detect_model_mismatch(&[session], &pricing).is_empty());
    }

    #[test]
    fn model_mismatch_ignores_opus_turns_mixed_with_heavier_tools() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-opus-5", 3000, 80, 0, 0, None)];
        let tool_call_log = vec![
            ToolCall { turn_index: 0, name: "Edit".into(), file_path: Some("/a.rs".into()) },
            ToolCall { turn_index: 0, name: "Task".into(), file_path: None },
        ];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        assert!(detect_model_mismatch(&[session], &pricing).is_empty());
    }

    #[test]
    fn model_mismatch_ignores_sonnet_turns() {
        let pricing = PricingTable::defaults();
        let turns = vec![turn("claude-sonnet-5", 3000, 80, 0, 0, None)];
        let tool_call_log = vec![ToolCall { turn_index: 0, name: "Edit".into(), file_path: Some("/a.rs".into()) }];
        let session = SessionStats { session_id: "s1".into(), turns, tool_call_log, ..Default::default() };
        assert!(detect_model_mismatch(&[session], &pricing).is_empty());
    }

    // --- percentile helper ---

    #[test]
    fn percentile_p95_of_100_values_is_the_95th() {
        let values: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!(percentile(&values, 0.95), Some(95.0));
    }

    #[test]
    fn percentile_of_empty_is_none() {
        assert_eq!(percentile(&[], 0.95), None);
    }
}
