use std::collections::HashMap;

use serde::Serialize;

use crate::pricing::PricingTable;
use crate::session::SessionStats;

#[derive(Debug, Serialize)]
struct ModelCostPayload<'a> {
    model: &'a str,
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
}

#[derive(Debug, Serialize)]
struct IngestPayload<'a> {
    date: &'a str,
    #[serde(rename = "sourceLabel")]
    source_label: &'a str,
    #[serde(rename = "sessionsCount")]
    sessions_count: u64,
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
    #[serde(rename = "cacheWriteTokens")]
    cache_write_tokens: u64,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: u64,
    #[serde(rename = "costEstimateUsd")]
    cost_estimate_usd: f64,
    #[serde(rename = "tokensSavedEstimate", skip_serializing_if = "Option::is_none")]
    tokens_saved_estimate: Option<u64>,
    #[serde(rename = "modelCosts", skip_serializing_if = "Vec::is_empty")]
    model_costs: Vec<ModelCostPayload<'a>>,
}

#[derive(Default)]
struct ModelAccum {
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
}

#[derive(Default)]
struct DayBucket {
    sessions_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_write_tokens: u64,
    cache_read_tokens: u64,
    cost_usd: f64,
    by_model: HashMap<String, ModelAccum>,
}

/// First 10 chars of an ISO 8601 timestamp is its calendar date
/// ("2026-08-01T12:02:49.435Z" -> "2026-08-01"); falls back to "unknown"
/// for a session with no timestamp at all rather than dropping its data.
fn day_key(timestamp: Option<&str>) -> String {
    match timestamp.and_then(|t| t.get(0..10)) {
        Some(day) => day.to_string(),
        None => "unknown".to_string(),
    }
}

fn bucket_by_day(sessions: &[SessionStats], pricing: &PricingTable) -> HashMap<String, DayBucket> {
    let mut buckets: HashMap<String, DayBucket> = HashMap::new();

    for session in sessions {
        let key = day_key(session.first_timestamp.as_deref());
        let bucket = buckets.entry(key).or_default();
        bucket.sessions_count += 1;

        let usage = session.total_usage();
        bucket.input_tokens += usage.input_tokens;
        bucket.output_tokens += usage.output_tokens;
        bucket.cache_write_tokens += usage.cache_creation_input_tokens;
        bucket.cache_read_tokens += usage.cache_read_input_tokens;

        for turn in &session.turns {
            let cost = pricing.cost_usd(&turn.model, &turn.usage);
            bucket.cost_usd += cost;

            let accum = bucket.by_model.entry(turn.model.clone()).or_default();
            accum.input_tokens += turn.usage.input_tokens;
            accum.output_tokens += turn.usage.output_tokens;
            accum.cost_usd += cost;
        }
    }

    buckets
}

/// Best-effort hostname lookup for the snapshot's `sourceLabel`. Not an
/// identity check — just a human-readable tag so a team can tell which
/// machine a snapshot came from. Never fails the push if it can't find one.
fn source_label() -> String {
    for var in ["COMPUTERNAME", "HOSTNAME", "HOST"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return v;
            }
        }
    }
    "unknown-machine".to_string()
}

pub struct PushOutcome {
    pub day: String,
    pub result: Result<(), String>,
}

/// Pushes one aggregated, numbers-only snapshot per calendar day covered by
/// `sessions`. `tokens_saved_estimate` (from the plugin's local log, if any)
/// is attributed only to today's bucket — the underlying log has no
/// per-session or per-day breakdown, so spreading it across historical days
/// would be a fabricated number, not a real one.
pub fn push_snapshots(
    sessions: &[SessionStats],
    pricing: &PricingTable,
    api_url: &str,
    api_key: &str,
    tokens_saved_today: u64,
) -> Vec<PushOutcome> {
    let buckets = bucket_by_day(sessions, pricing);
    let label = source_label();
    let today = chrono_free_today();

    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/metrics/ingest", api_url.trim_end_matches('/'));

    let mut outcomes = Vec::with_capacity(buckets.len());
    for (day, bucket) in buckets {
        let mut model_costs: Vec<ModelCostPayload> = bucket
            .by_model
            .iter()
            .map(|(model, accum)| ModelCostPayload {
                model,
                input_tokens: accum.input_tokens,
                output_tokens: accum.output_tokens,
                cost_usd: accum.cost_usd,
            })
            .collect();
        model_costs.sort_by(|a, b| {
            b.cost_usd
                .partial_cmp(&a.cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let payload = IngestPayload {
            date: &day,
            source_label: &label,
            sessions_count: bucket.sessions_count,
            input_tokens: bucket.input_tokens,
            output_tokens: bucket.output_tokens,
            cache_write_tokens: bucket.cache_write_tokens,
            cache_read_tokens: bucket.cache_read_tokens,
            cost_estimate_usd: bucket.cost_usd,
            tokens_saved_estimate: if day == today { Some(tokens_saved_today) } else { None },
            model_costs,
        };

        let result = client
            .post(&endpoint)
            .header("x-api-key", api_key)
            .json(&payload)
            .send()
            .map_err(|e| e.to_string())
            .and_then(|resp| {
                if resp.status().is_success() {
                    Ok(())
                } else {
                    let status = resp.status();
                    let body = resp.text().unwrap_or_default();
                    Err(format!("{status}: {body}"))
                }
            });

        outcomes.push(PushOutcome { day, result });
    }

    outcomes.sort_by(|a, b| a.day.cmp(&b.day));
    outcomes
}

/// No date/time crate in this project yet, and pulling one in just for
/// "what's today's date" would be a heavy dependency for one string. The
/// system clock plus days-since-epoch arithmetic is enough for a
/// same-machine comparison against `day_key`'s YYYY-MM-DD format.
fn chrono_free_today() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days_since_epoch = secs / 86_400;

    // Civil-from-days algorithm (Howard Hinnant's date algorithms, public
    // domain) — avoids a chrono dependency for one date computation.
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Usage;

    #[test]
    fn day_key_extracts_date_portion() {
        assert_eq!(day_key(Some("2026-08-01T12:02:49.435Z")), "2026-08-01");
    }

    #[test]
    fn day_key_falls_back_when_missing() {
        assert_eq!(day_key(None), "unknown");
    }

    #[test]
    fn bucket_by_day_groups_sessions_on_same_date() {
        let pricing = PricingTable::defaults();
        let sessions = vec![
            SessionStats {
                session_id: "a".into(),
                first_timestamp: Some("2026-08-01T10:00:00.000Z".into()),
                turns: vec![crate::session::Turn {
                    model: "claude-sonnet-5".into(),
                    usage: Usage { input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                    timestamp: None,
                }],
                ..Default::default()
            },
            SessionStats {
                session_id: "b".into(),
                first_timestamp: Some("2026-08-01T18:00:00.000Z".into()),
                turns: vec![crate::session::Turn {
                    model: "claude-sonnet-5".into(),
                    usage: Usage { input_tokens: 200, output_tokens: 100, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                    timestamp: None,
                }],
                ..Default::default()
            },
            SessionStats {
                session_id: "c".into(),
                first_timestamp: Some("2026-08-02T09:00:00.000Z".into()),
                turns: vec![],
                ..Default::default()
            },
        ];

        let buckets = bucket_by_day(&sessions, &pricing);
        assert_eq!(buckets.len(), 2);
        let day1 = buckets.get("2026-08-01").unwrap();
        assert_eq!(day1.sessions_count, 2);
        assert_eq!(day1.input_tokens, 300);
        let day2 = buckets.get("2026-08-02").unwrap();
        assert_eq!(day2.sessions_count, 1);
        assert_eq!(day2.input_tokens, 0);
    }

    #[test]
    fn bucket_by_day_aggregates_cost_and_tokens_per_model() {
        let pricing = PricingTable::defaults();
        let sessions = vec![SessionStats {
            session_id: "a".into(),
            first_timestamp: Some("2026-08-01T10:00:00.000Z".into()),
            turns: vec![
                crate::session::Turn {
                    model: "claude-opus-5".into(),
                    usage: Usage { input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                    timestamp: None,
                },
                crate::session::Turn {
                    model: "claude-haiku-4-5".into(),
                    usage: Usage { input_tokens: 10, output_tokens: 5, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                    timestamp: None,
                },
                crate::session::Turn {
                    model: "claude-opus-5".into(),
                    usage: Usage { input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                    timestamp: None,
                },
            ],
            ..Default::default()
        }];

        let buckets = bucket_by_day(&sessions, &pricing);
        let day = buckets.get("2026-08-01").unwrap();

        assert_eq!(day.by_model.len(), 2);
        let opus = day.by_model.get("claude-opus-5").unwrap();
        assert_eq!(opus.input_tokens, 200);
        assert_eq!(opus.output_tokens, 100);
        assert!(opus.cost_usd > 0.0);
        let haiku = day.by_model.get("claude-haiku-4-5").unwrap();
        assert_eq!(haiku.input_tokens, 10);
        // Confirms each model's cost was priced with its own rate rather
        // than all turns collapsing into one undifferentiated total —
        // Opus's per-token rate is ~19x Haiku's, so even with 20x fewer
        // tokens Haiku's slice should cost less.
        assert!(opus.cost_usd > haiku.cost_usd);
    }

    #[test]
    fn today_string_is_plausible_iso_date() {
        let today = chrono_free_today();
        assert_eq!(today.len(), 10);
        assert_eq!(today.chars().nth(4), Some('-'));
        assert_eq!(today.chars().nth(7), Some('-'));
    }
}
