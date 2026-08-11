use std::collections::HashMap;
use std::io::{BufRead, BufReader};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    hook: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tokens_saved_estimate: Option<u64>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    command: Option<String>,
}

/// One line of `~/.claude/contextguard/savings.jsonl`, parsed. `session_id`
/// and `command` are only present on entries logged by a plugin build new
/// enough to write them (see contextguard-plugin's savings-log.js) — older
/// entries still parse, just without the fields that make amortization and
/// top-source ranking possible for them.
#[derive(Debug, Clone)]
pub struct SavingsEntry {
    pub timestamp: Option<String>,
    pub hook: Option<String>,
    pub tool_name: Option<String>,
    pub tokens_saved_estimate: Option<u64>,
    pub session_id: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Default)]
pub struct SavingsReport {
    pub interventions: u64,
    pub tokens_saved_estimate: u64,
    pub by_hook: HashMap<String, u64>,
}

fn log_path() -> Option<std::path::PathBuf> {
    Some(dirs::home_dir()?.join(".claude").join("contextguard").join("savings.jsonl"))
}

/// Reads every entry the plugin's hooks have logged, oldest first. Empty —
/// not an error — if the plugin was never installed or nothing has fired
/// yet.
pub fn read_entries() -> Vec<SavingsEntry> {
    let mut entries = Vec::new();

    let Some(path) = log_path() else { return entries };
    let Ok(file) = std::fs::File::open(path) else { return entries };
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(raw) = serde_json::from_str::<RawEntry>(&line) else { continue };
        entries.push(SavingsEntry {
            timestamp: raw.ts,
            hook: raw.hook,
            tool_name: raw.tool_name,
            tokens_saved_estimate: raw.tokens_saved_estimate,
            session_id: raw.session_id,
            command: raw.command,
        });
    }

    entries
}

/// The flat lifetime summary the headline report's one-line "plugin is
/// already saving tokens" mention uses. `savings_report::build` is the
/// richer, monthly, amortized breakdown built on the same entries.
pub fn read() -> SavingsReport {
    let mut report = SavingsReport::default();
    for entry in read_entries() {
        report.interventions += 1;
        if let Some(tokens) = entry.tokens_saved_estimate {
            report.tokens_saved_estimate += tokens;
        }
        if let Some(hook) = entry.hook {
            *report.by_hook.entry(hook).or_insert(0) += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn parse_lines(lines: &[&str]) -> Vec<SavingsEntry> {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        let reader = BufReader::new(std::fs::File::open(file.path()).unwrap());
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap();
            if line.trim().is_empty() {
                continue;
            }
            let Ok(raw) = serde_json::from_str::<RawEntry>(&line) else { continue };
            entries.push(SavingsEntry {
                timestamp: raw.ts,
                hook: raw.hook,
                tool_name: raw.tool_name,
                tokens_saved_estimate: raw.tokens_saved_estimate,
                session_id: raw.session_id,
                command: raw.command,
            });
        }
        entries
    }

    fn summarize(entries: &[SavingsEntry]) -> SavingsReport {
        let mut report = SavingsReport::default();
        for entry in entries {
            report.interventions += 1;
            if let Some(tokens) = entry.tokens_saved_estimate {
                report.tokens_saved_estimate += tokens;
            }
            if let Some(hook) = &entry.hook {
                *report.by_hook.entry(hook.clone()).or_insert(0) += 1;
            }
        }
        report
    }

    #[test]
    fn sums_tokens_saved_across_entries() {
        let report = summarize(&parse_lines(&[
            r#"{"ts":"t","hook":"bash_truncate","tool_name":"Bash","tokens_saved_estimate":100}"#,
            r#"{"ts":"t","hook":"bash_truncate","tool_name":"Bash","tokens_saved_estimate":50}"#,
        ]));
        assert_eq!(report.interventions, 2);
        assert_eq!(report.tokens_saved_estimate, 150);
    }

    #[test]
    fn entries_without_token_estimate_still_count_as_interventions() {
        let report = summarize(&parse_lines(&[r#"{"ts":"t","hook":"grep_cap","tool_name":"Grep","capped_to":100}"#]));
        assert_eq!(report.interventions, 1);
        assert_eq!(report.tokens_saved_estimate, 0);
        assert_eq!(report.by_hook.get("grep_cap"), Some(&1));
    }

    #[test]
    fn ignores_malformed_lines() {
        let entries = parse_lines(&["not json", r#"{"hook":"bash_truncate","tokens_saved_estimate":10}"#]);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn reads_session_id_and_command_when_present() {
        let entries = parse_lines(&[
            r#"{"ts":"t","hook":"bash_truncate","tokens_saved_estimate":10,"session_id":"s1","command":"npm test"}"#,
        ]);
        assert_eq!(entries[0].session_id.as_deref(), Some("s1"));
        assert_eq!(entries[0].command.as_deref(), Some("npm test"));
    }

    #[test]
    fn tolerates_older_entries_missing_session_id_and_command() {
        let entries = parse_lines(&[r#"{"ts":"t","hook":"bash_truncate","tokens_saved_estimate":10}"#]);
        assert!(entries[0].session_id.is_none());
        assert!(entries[0].command.is_none());
    }
}
