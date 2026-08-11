//! `contextguard budget` — a local spend-threshold check.
//!
//! Fully local: sums cost from the session transcripts already on disk for
//! the requested period (today, or the current calendar month) and compares
//! it against `--max`. No account, no `--push`, no network call unless
//! `--webhook-url` is explicitly given — the same "local by default"
//! posture as the rest of this tool.
//!
//! This is a different mechanism from the dashboard's own team-level budget
//! alerts (contextguard-api's budget-alert-webhook.service.ts), which
//! require an account and `--push`'d data and fire from the server. This
//! one works for someone who has never pushed anything anywhere: exit
//! nonzero when the threshold is crossed, so it can gate a script, and post
//! the same Slack/Discord-compatible payload the dashboard sends
//! (`{"text": "..."}` — Discord accepts this on a webhook URL with `/slack`
//! appended, same as the dashboard's own setup instructions say) when a
//! webhook URL is supplied.

use crate::pricing::PricingTable;
use crate::push::{chrono_free_today, day_key};
use crate::session::SessionStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Daily,
    Monthly,
}

#[derive(Debug)]
pub struct BudgetCheck {
    pub period: Period,
    /// "2026-08-11" for Daily, "2026-08" for Monthly — what every turn's
    /// own day_key was compared against.
    pub period_key: String,
    pub spend_usd: f64,
    pub max_usd: f64,
}

impl BudgetCheck {
    pub fn crossed(&self) -> bool {
        self.spend_usd >= self.max_usd
    }
}

/// `today` is injected rather than computed inside — lets a test pin "now"
/// instead of depending on the system clock, the same reason
/// `detect_burn_rate` and the CLAUDE.md amortizer take sessions rather than
/// reaching for `SystemTime::now()` themselves.
pub fn check(sessions: &[SessionStats], pricing: &PricingTable, period: Period, max_usd: f64, today: &str) -> BudgetCheck {
    let period_key = match period {
        Period::Daily => today.to_string(),
        // "2026-08-11" -> "2026-08". `today` is always produced by
        // chrono_free_today() in this exact shape, so the slice is safe by
        // construction rather than needing a fallback.
        Period::Monthly => today.get(0..7).unwrap_or(today).to_string(),
    };

    let mut spend_usd = 0.0;
    for session in sessions {
        for turn in &session.turns {
            let day = day_key(turn.timestamp.as_deref());
            let in_period = match period {
                Period::Daily => day == period_key,
                Period::Monthly => day.starts_with(&period_key),
            };
            if in_period {
                spend_usd += pricing.cost_usd(&turn.model, &turn.usage);
            }
        }
    }

    BudgetCheck { period, period_key, spend_usd, max_usd }
}

pub fn check_today(sessions: &[SessionStats], pricing: &PricingTable, period: Period, max_usd: f64) -> BudgetCheck {
    check(sessions, pricing, period, max_usd, &chrono_free_today())
}

/// Posts the same `{"text": ...}` payload the dashboard's own budget-alert
/// webhook sends — Slack accepts it natively, Discord accepts it on a
/// webhook URL with `/slack` appended (see contextguard-web's Budget Alerts
/// page, which tells users the same thing). A delivery failure is reported
/// to the caller to print, not treated as the check itself failing — the
/// budget verdict is real regardless of whether the notification made it
/// out.
pub fn notify_webhook(url: &str, check: &BudgetCheck) -> Result<(), String> {
    let period_label = match check.period {
        Period::Daily => "today",
        Period::Monthly => "this month",
    };
    let text = format!(
        ":rotating_light: ContextGuard budget alert: spent ${:.2} {period_label}, over the ${:.2} threshold.",
        check.spend_usd, check.max_usd
    );

    let client = reqwest::blocking::Client::new();
    let res = client
        .post(url)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("webhook endpoint returned {}", res.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Turn, Usage};

    fn turn(model: &str, ts: &str, input_tokens: u64) -> Turn {
        Turn { model: model.to_string(), usage: Usage { input_tokens, ..Default::default() }, timestamp: Some(ts.to_string()) }
    }

    fn session(turns: Vec<Turn>) -> SessionStats {
        SessionStats { session_id: "s".into(), turns, ..Default::default() }
    }

    #[test]
    fn sums_only_turns_within_todays_daily_period() {
        let sessions = vec![session(vec![
            turn("claude-sonnet-5", "2026-08-11T09:00:00.000Z", 1_000_000),
            turn("claude-sonnet-5", "2026-08-10T09:00:00.000Z", 1_000_000), // yesterday — excluded
        ])];
        let result = check(&sessions, &PricingTable::defaults(), Period::Daily, 100.0, "2026-08-11");
        assert!((result.spend_usd - 3.0).abs() < 1e-9); // 1M sonnet input tokens = $3
    }

    #[test]
    fn monthly_period_includes_every_day_in_the_calendar_month() {
        let sessions = vec![session(vec![
            turn("claude-sonnet-5", "2026-08-01T00:00:00.000Z", 1_000_000),
            turn("claude-sonnet-5", "2026-08-31T00:00:00.000Z", 1_000_000),
            turn("claude-sonnet-5", "2026-07-31T00:00:00.000Z", 1_000_000), // last month — excluded
        ])];
        let result = check(&sessions, &PricingTable::defaults(), Period::Monthly, 100.0, "2026-08-15");
        assert!((result.spend_usd - 6.0).abs() < 1e-9);
    }

    #[test]
    fn crossed_is_true_at_or_above_the_threshold_not_only_strictly_above() {
        let sessions = vec![session(vec![turn("claude-sonnet-5", "2026-08-11T00:00:00.000Z", 1_000_000)])];
        let result = check(&sessions, &PricingTable::defaults(), Period::Daily, 3.0, "2026-08-11");
        assert!(result.crossed());
    }

    #[test]
    fn under_threshold_is_not_crossed() {
        let sessions = vec![session(vec![turn("claude-sonnet-5", "2026-08-11T00:00:00.000Z", 1_000_000)])];
        let result = check(&sessions, &PricingTable::defaults(), Period::Daily, 3.01, "2026-08-11");
        assert!(!result.crossed());
    }

    #[test]
    fn zero_sessions_is_zero_spend_not_an_error() {
        let result = check(&[], &PricingTable::defaults(), Period::Monthly, 50.0, "2026-08-11");
        assert_eq!(result.spend_usd, 0.0);
        assert!(!result.crossed());
    }
}
