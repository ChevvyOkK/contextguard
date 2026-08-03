use std::collections::HashMap;
use std::io::{BufRead, BufReader};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    hook: Option<String>,
    #[serde(default)]
    tokens_saved_estimate: Option<u64>,
}

#[derive(Debug, Default)]
pub struct SavingsReport {
    pub interventions: u64,
    pub tokens_saved_estimate: u64,
    pub by_hook: HashMap<String, u64>,
}

/// Reads the local intervention log the ContextGuard plugin's hooks append
/// to (`~/.claude/contextguard/savings.jsonl`). Absent entirely if the
/// plugin was never installed — that's a normal, silent no-op, not an error.
pub fn read() -> SavingsReport {
    let mut report = SavingsReport::default();

    let Some(home) = dirs::home_dir() else { return report };
    let path = home.join(".claude").join("contextguard").join("savings.jsonl");
    let Ok(file) = std::fs::File::open(path) else { return report };
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<RawEntry>(&line) else { continue };

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

    fn parse_lines(lines: &[&str]) -> SavingsReport {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        let reader = BufReader::new(std::fs::File::open(file.path()).unwrap());
        let mut report = SavingsReport::default();
        for line in reader.lines() {
            let line = line.unwrap();
            if line.trim().is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<RawEntry>(&line) else { continue };
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

    #[test]
    fn sums_tokens_saved_across_entries() {
        let report = parse_lines(&[
            r#"{"ts":"t","hook":"bash_truncate","tool_name":"Bash","tokens_saved_estimate":100}"#,
            r#"{"ts":"t","hook":"bash_truncate","tool_name":"Bash","tokens_saved_estimate":50}"#,
        ]);
        assert_eq!(report.interventions, 2);
        assert_eq!(report.tokens_saved_estimate, 150);
    }

    #[test]
    fn entries_without_token_estimate_still_count_as_interventions() {
        let report = parse_lines(&[r#"{"ts":"t","hook":"grep_cap","tool_name":"Grep","capped_to":100}"#]);
        assert_eq!(report.interventions, 1);
        assert_eq!(report.tokens_saved_estimate, 0);
        assert_eq!(report.by_hook.get("grep_cap"), Some(&1));
    }

    #[test]
    fn ignores_malformed_lines() {
        let report = parse_lines(&["not json", r#"{"hook":"bash_truncate","tokens_saved_estimate":10}"#]);
        assert_eq!(report.interventions, 1);
    }
}
