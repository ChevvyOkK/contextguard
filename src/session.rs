use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

use crate::i18n::{self, Lang};

#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }
}

#[derive(Debug, Default)]
pub struct SessionStats {
    pub session_id: String,
    pub cwd: Option<String>,
    /// (model, usage) per assistant turn — kept granular so cost can be
    /// computed per-model rather than assuming one model for the session.
    pub turns: Vec<(String, Usage)>,
    pub tool_calls: HashMap<String, u64>,
    /// ISO 8601 timestamp of the first record seen in the file. Used to
    /// bucket a session into a calendar day for `--push`; a session that
    /// spans midnight is attributed entirely to its start day rather than
    /// split, which is a deliberate simplification, not an oversight.
    pub first_timestamp: Option<String>,
}

impl SessionStats {
    pub fn total_usage(&self) -> Usage {
        let mut total = Usage::default();
        for (_, u) in &self.turns {
            total.add(u);
        }
        total
    }
}

// Only the fields we care about; every record has a variable, mostly-
// irrelevant-to-us shape, so we deserialize into a permissive shell rather
// than modeling the entire transcript format.
#[derive(Debug, Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<RawMessage>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<RawUsage>,
    #[serde(default)]
    content: Option<Vec<RawContentBlock>>,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct RawContentBlock {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

pub fn parse_session_file(path: &Path, lang: Lang) -> Result<SessionStats, String> {
    let file = File::open(path).map_err(|e| i18n::err_open_file(lang, &format!("{path:?}"), &e.to_string()))?;
    let reader = BufReader::new(file);

    let session_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
    let mut stats = SessionStats { session_id, ..Default::default() };

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(raw) = serde_json::from_str::<RawLine>(&line) else { continue };

        if stats.cwd.is_none() {
            stats.cwd = raw.cwd;
        }
        if stats.first_timestamp.is_none() {
            stats.first_timestamp = raw.timestamp;
        }

        if raw.kind.as_deref() != Some("assistant") {
            continue;
        }
        let Some(message) = raw.message else { continue };

        if let Some(u) = &message.usage {
            let model = message.model.clone().unwrap_or_else(|| "unknown".to_string());
            stats.turns.push((
                model,
                Usage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cache_creation_input_tokens: u.cache_creation_input_tokens,
                    cache_read_input_tokens: u.cache_read_input_tokens,
                },
            ));
        }

        if let Some(blocks) = &message.content {
            for block in blocks {
                if block.kind.as_deref() == Some("tool_use") {
                    if let Some(name) = &block.name {
                        *stats.tool_calls.entry(name.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::with_suffix(".jsonl").unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    #[test]
    fn extracts_usage_from_assistant_messages() {
        let file = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"assistant","cwd":"/home/x","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":5,"cache_read_input_tokens":100}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns.len(), 1);
        let total = stats.total_usage();
        assert_eq!(total.input_tokens, 10);
        assert_eq!(total.output_tokens, 20);
        assert_eq!(total.cache_read_input_tokens, 100);
        assert_eq!(stats.cwd.as_deref(), Some("/home/x"));
    }

    #[test]
    fn counts_tool_use_by_name() {
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[{"type":"tool_use","name":"Read"},{"type":"tool_use","name":"Read"},{"type":"tool_use","name":"Bash"}]}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.tool_calls.get("Read"), Some(&2));
        assert_eq!(stats.tool_calls.get("Bash"), Some(&1));
    }

    #[test]
    fn ignores_malformed_lines_without_failing_the_whole_file() {
        let file = write_jsonl(&[
            "not even json",
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":5,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns.len(), 1);
    }

    #[test]
    fn non_assistant_lines_do_not_contribute_usage() {
        let file = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"hello","usage":{"input_tokens":999,"output_tokens":999,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns.len(), 0);
    }

    #[test]
    fn multiple_turns_accumulate_total_usage() {
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":20,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        let total = stats.total_usage();
        assert_eq!(total.input_tokens, 30);
        assert_eq!(total.output_tokens, 30);
    }
}
