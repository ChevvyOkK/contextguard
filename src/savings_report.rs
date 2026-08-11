//! `contextguard savings` — what the plugin actually saved, priced honestly.
//!
//! The bash-truncate hook doesn't estimate its savings: it measures the
//! exact character delta between the tool output Claude Code would
//! otherwise have received and the smaller version it actually wrote to
//! the transcript, before either one is ever sent to the API. That number
//! (`tokens_saved_estimate` in savings.jsonl) is already a real
//! measurement — nothing here has to guess at a counterfactual by running
//! anything twice.
//!
//! What this module adds is amortization. The truncated output is what
//! gets cached and resent on every subsequent turn of that session, not a
//! one-time saving — so a truncation on turn 3 of a 40-turn session is
//! worth far more than the same truncation on the last turn. When the
//! entry's `session_id` matches a locally-parsed session, this prices it
//! by how many turns were still ahead of it (the same "carried tokens"
//! idea src/context.rs uses for CLAUDE.md and tool results). When it
//! doesn't — an older plugin build that never logged a session_id, or the
//! session has since rotated out of the `--days` window — this falls back
//! to counting the saving once, the same honest floor used throughout this
//! tool rather than a guess at what the multiplier should have been.
//!
//! The Grep cap hook is reported separately and never priced at all: it
//! fires before the search runs, so there is nothing yet to measure a
//! delta against (see cap-grep-limit.js's own comment on this).

use std::collections::HashMap;

use crate::pricing::PricingTable;
use crate::savings::{read_entries, SavingsEntry};
use crate::session::SessionStats;
use crate::timeutil;

#[derive(Debug, Clone)]
pub struct TopCommand {
    pub label: String,
    pub tokens: u64,
}

#[derive(Debug, Clone)]
pub struct MonthlySavings {
    /// "2026-08"
    pub month: String,
    /// Raw (unamortized) tokens the truncate hook measured this month.
    pub bash_truncate_tokens: u64,
    /// Amortized dollar value: tokens carried across the turns still ahead
    /// of each intervention, priced at the session's own model.
    pub bash_truncate_usd: f64,
    /// How many of this month's truncate interventions had a session_id
    /// that matched a session in the window analyzed — i.e. how many were
    /// actually amortized versus counted once for lack of data.
    pub amortized_interventions: u64,
    pub total_bash_interventions: u64,
    pub top_command: Option<TopCommand>,
    pub grep_cap_interventions: u64,
    pub sessions_touched: usize,
}

/// All months with at least one intervention, most recent first.
pub fn build(sessions: &[SessionStats], pricing: &PricingTable) -> Vec<MonthlySavings> {
    let entries = read_entries();
    build_from_entries(&entries, sessions, pricing)
}

fn build_from_entries(entries: &[SavingsEntry], sessions: &[SessionStats], pricing: &PricingTable) -> Vec<MonthlySavings> {
    let by_session: HashMap<&str, &SessionStats> =
        sessions.iter().map(|s| (s.session_id.as_str(), s)).collect();

    struct MonthAcc {
        bash_truncate_tokens: u64,
        bash_truncate_usd: f64,
        amortized_interventions: u64,
        total_bash_interventions: u64,
        by_command: HashMap<String, u64>,
        grep_cap_interventions: u64,
        sessions: std::collections::HashSet<String>,
    }
    impl Default for MonthAcc {
        fn default() -> Self {
            MonthAcc {
                bash_truncate_tokens: 0,
                bash_truncate_usd: 0.0,
                amortized_interventions: 0,
                total_bash_interventions: 0,
                by_command: HashMap::new(),
                grep_cap_interventions: 0,
                sessions: std::collections::HashSet::new(),
            }
        }
    }

    let mut months: HashMap<String, MonthAcc> = HashMap::new();

    for entry in entries {
        let Some(ts) = &entry.timestamp else { continue };
        // "2026-08-11T14:18:20.150Z" -> "2026-08". A timestamp too short or
        // malformed to slice safely is dropped rather than mis-bucketed —
        // see_parse_epoch_seconds's own "no signal, not an error" policy.
        if ts.len() < 7 {
            continue;
        }
        let month = ts[..7].to_string();
        let acc = months.entry(month).or_default();

        match entry.hook.as_deref() {
            Some("grep_cap") => {
                acc.grep_cap_interventions += 1;
                if let Some(sid) = &entry.session_id {
                    acc.sessions.insert(sid.clone());
                }
            }
            Some("bash_truncate") | None => {
                let Some(tokens) = entry.tokens_saved_estimate else { continue };
                acc.total_bash_interventions += 1;
                acc.bash_truncate_tokens += tokens;

                let (usd, amortized) = price_intervention(entry, tokens, &by_session, pricing);
                acc.bash_truncate_usd += usd;
                if amortized {
                    acc.amortized_interventions += 1;
                }

                let label = entry.command.clone().unwrap_or_else(|| {
                    entry.tool_name.clone().unwrap_or_else(|| "Bash".to_string())
                });
                *acc.by_command.entry(label).or_insert(0) += tokens;

                if let Some(sid) = &entry.session_id {
                    acc.sessions.insert(sid.clone());
                }
            }
            Some(_) => {}
        }
    }

    let mut out: Vec<MonthlySavings> = months
        .into_iter()
        .map(|(month, acc)| MonthlySavings {
            month,
            bash_truncate_tokens: acc.bash_truncate_tokens,
            bash_truncate_usd: acc.bash_truncate_usd,
            amortized_interventions: acc.amortized_interventions,
            total_bash_interventions: acc.total_bash_interventions,
            top_command: acc
                .by_command
                .into_iter()
                .max_by_key(|(_, tokens)| *tokens)
                .map(|(label, tokens)| TopCommand { label, tokens }),
            grep_cap_interventions: acc.grep_cap_interventions,
            sessions_touched: acc.sessions.len(),
        })
        .collect();

    out.sort_by(|a, b| b.month.cmp(&a.month));
    out
}

/// Dollar value of one truncation, and whether it was priced against a real
/// session (amortized) or just counted once (no matching session data).
fn price_intervention(
    entry: &SavingsEntry,
    tokens: u64,
    by_session: &HashMap<&str, &SessionStats>,
    pricing: &PricingTable,
) -> (f64, bool) {
    let fallback = |tokens: u64| -> f64 {
        // No session to price against: value it at the blended Sonnet
        // cache-read rate (what re-sending it would cost) for exactly one
        // carry — the floor, not a guess at how many more turns it saw.
        (tokens as f64 / 1_000_000.0) * pricing.for_model("claude-sonnet-5").cache_read_per_mtok
    };

    let Some(session_id) = &entry.session_id else { return (fallback(tokens), false) };
    let Some(session) = by_session.get(session_id.as_str()) else { return (fallback(tokens), false) };
    let Some(ts) = &entry.timestamp else { return (fallback(tokens), false) };
    let Some(ts_epoch) = timeutil::parse_epoch_seconds(ts) else { return (fallback(tokens), false) };

    let total_turns = session.turns.len();
    if total_turns == 0 {
        return (fallback(tokens), false);
    }

    // The first turn whose own timestamp is at or after this intervention —
    // that's the turn whose request first carried the truncated (smaller)
    // output. Every turn from there on re-sends it from cache. No turn
    // found at/after it (the intervention landed after the last recorded
    // turn) falls back to the last turn, which is still a real carry of 1
    // rather than nothing.
    let turn_index = session
        .turns
        .iter()
        .position(|t| t.timestamp.as_deref().and_then(timeutil::parse_epoch_seconds).is_some_and(|e| e >= ts_epoch))
        .unwrap_or(total_turns - 1);

    let carries = (total_turns - turn_index).max(1) as u64;
    let rate = session
        .turns
        .get(turn_index)
        .map(|t| pricing.for_model(&t.model).cache_read_per_mtok)
        .unwrap_or_else(|| pricing.for_model("claude-sonnet-5").cache_read_per_mtok);

    let usd = (tokens * carries) as f64 / 1_000_000.0 * rate;
    (usd, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Turn;

    fn entry(ts: &str, hook: &str, tokens: Option<u64>, session_id: Option<&str>, command: Option<&str>) -> SavingsEntry {
        SavingsEntry {
            timestamp: Some(ts.to_string()),
            hook: Some(hook.to_string()),
            tool_name: Some("Bash".to_string()),
            tokens_saved_estimate: tokens,
            session_id: session_id.map(str::to_string),
            command: command.map(str::to_string),
        }
    }

    fn session_with_turns(id: &str, timestamps: &[&str]) -> SessionStats {
        SessionStats {
            session_id: id.to_string(),
            turns: timestamps
                .iter()
                .map(|ts| Turn { model: "claude-sonnet-5".into(), usage: Default::default(), timestamp: Some(ts.to_string()) })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn groups_entries_by_calendar_month() {
        let entries = vec![
            entry("2026-07-15T00:00:00.000Z", "bash_truncate", Some(100), None, None),
            entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(200), None, None),
        ];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());
        assert_eq!(months.len(), 2);
        assert_eq!(months[0].month, "2026-08"); // most recent first
        assert_eq!(months[1].month, "2026-07");
    }

    #[test]
    fn amortizes_an_early_intervention_across_the_turns_that_followed_it() {
        let session = session_with_turns(
            "s1",
            &[
                "2026-08-01T00:00:00.000Z",
                "2026-08-01T00:05:00.000Z",
                "2026-08-01T00:10:00.000Z",
                "2026-08-01T00:15:00.000Z",
                "2026-08-01T00:20:00.000Z",
            ],
        );
        // Fires right before turn 0 (5 turns still ahead, including it).
        let entries = vec![entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(1000), Some("s1"), None)];
        let months = build_from_entries(&entries, &[session], &PricingTable::defaults());

        assert_eq!(months[0].amortized_interventions, 1);
        // 1000 tokens * 5 carries at Sonnet's $0.30/mtok cache-read rate.
        assert!((months[0].bash_truncate_usd - (1000.0 * 5.0 / 1_000_000.0 * 0.30)).abs() < 1e-9);
    }

    #[test]
    fn falls_back_to_counting_once_with_no_matching_session() {
        let entries = vec![entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(1000), Some("unknown"), None)];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());

        assert_eq!(months[0].amortized_interventions, 0);
        assert!((months[0].bash_truncate_usd - (1000.0 / 1_000_000.0 * 0.30)).abs() < 1e-9);
    }

    #[test]
    fn falls_back_to_counting_once_with_no_session_id_at_all() {
        // The common real-world case today: entries logged before the
        // plugin started writing session_id.
        let entries = vec![entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(500), None, None)];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());

        assert_eq!(months[0].amortized_interventions, 0);
        assert_eq!(months[0].total_bash_interventions, 1);
        assert!(months[0].bash_truncate_usd > 0.0);
    }

    #[test]
    fn ranks_the_top_command_by_tokens_not_by_count() {
        let entries = vec![
            entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(100), None, Some("pytest")),
            entry("2026-08-02T00:00:00.000Z", "bash_truncate", Some(100), None, Some("pytest")),
            entry("2026-08-03T00:00:00.000Z", "bash_truncate", Some(5000), None, Some("npm test")),
        ];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());
        let top = months[0].top_command.as_ref().unwrap();
        assert_eq!(top.label, "npm test");
        assert_eq!(top.tokens, 5000);
    }

    #[test]
    fn reports_grep_cap_interventions_with_no_dollar_figure() {
        let entries = vec![SavingsEntry {
            timestamp: Some("2026-08-01T00:00:00.000Z".to_string()),
            hook: Some("grep_cap".to_string()),
            tool_name: Some("Grep".to_string()),
            tokens_saved_estimate: None,
            session_id: None,
            command: None,
        }];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());
        assert_eq!(months[0].grep_cap_interventions, 1);
        assert_eq!(months[0].bash_truncate_usd, 0.0);
    }

    #[test]
    fn drops_entries_with_no_timestamp_rather_than_guessing_a_month() {
        let entries = vec![SavingsEntry {
            timestamp: None,
            hook: Some("bash_truncate".to_string()),
            tool_name: None,
            tokens_saved_estimate: Some(100),
            session_id: None,
            command: None,
        }];
        assert!(build_from_entries(&entries, &[], &PricingTable::defaults()).is_empty());
    }

    #[test]
    fn counts_distinct_sessions_touched_this_month() {
        let entries = vec![
            entry("2026-08-01T00:00:00.000Z", "bash_truncate", Some(10), Some("a"), None),
            entry("2026-08-02T00:00:00.000Z", "bash_truncate", Some(10), Some("a"), None),
            entry("2026-08-03T00:00:00.000Z", "bash_truncate", Some(10), Some("b"), None),
        ];
        let months = build_from_entries(&entries, &[], &PricingTable::defaults());
        assert_eq!(months[0].sessions_touched, 2);
    }

    #[test]
    fn an_empty_log_produces_no_months() {
        assert!(build_from_entries(&[], &[], &PricingTable::defaults()).is_empty());
    }
}
