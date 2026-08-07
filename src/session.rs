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

/// One assistant turn: which model answered, what it cost in tokens, and
/// when — the timestamp is what lets the optimization engine (src/optimize.rs)
/// measure real elapsed time (burn rate) and turn-over-turn context growth.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub model: String,
    pub usage: Usage,
    pub timestamp: Option<String>,
}

/// A single tool_use block, kept separate from the per-name `tool_calls`
/// tally so the optimization engine can ask "which turn, which path" —
/// questions the aggregate count can't answer (e.g. the Re-Read Detector
/// needs to group Read calls by file path within a session).
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Index into `SessionStats::turns` for the turn this call happened in.
    /// Best-effort for a message with content but no usage block (rare,
    /// malformed data): attributed to the next turn boundary rather than
    /// dropped.
    pub turn_index: usize,
    pub name: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Default)]
pub struct SessionStats {
    pub session_id: String,
    pub cwd: Option<String>,
    pub turns: Vec<Turn>,
    pub tool_calls: HashMap<String, u64>,
    pub tool_call_log: Vec<ToolCall>,
    /// ISO 8601 timestamp of the first record seen in the file. Used to
    /// bucket a session into a calendar day for `--push`; a session that
    /// spans midnight is attributed entirely to its start day rather than
    /// split, which is a deliberate simplification, not an oversight.
    pub first_timestamp: Option<String>,
}

impl SessionStats {
    pub fn total_usage(&self) -> Usage {
        let mut total = Usage::default();
        for turn in &self.turns {
            total.add(&turn.usage);
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
    #[serde(default)]
    input: Option<RawToolInput>,
}

/// Tool call arguments, permissively parsed: only `file_path` is used
/// (Read/Edit/Write all key it the same way), everything else in `input`
/// is irrelevant here and never even reaches this struct.
#[derive(Debug, Deserialize)]
struct RawToolInput {
    #[serde(default)]
    file_path: Option<String>,
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
            stats.first_timestamp = raw.timestamp.clone();
        }

        if raw.kind.as_deref() != Some("assistant") {
            continue;
        }
        let Some(message) = raw.message else { continue };

        // Captured before pushing this line's turn (if any) so a tool_use
        // block in the same message points at the turn it actually belongs
        // to, not the one after it.
        let turn_index = stats.turns.len();

        if let Some(u) = &message.usage {
            let model = message.model.clone().unwrap_or_else(|| "unknown".to_string());
            stats.turns.push(Turn {
                model,
                usage: Usage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cache_creation_input_tokens: u.cache_creation_input_tokens,
                    cache_read_input_tokens: u.cache_read_input_tokens,
                },
                timestamp: raw.timestamp.clone(),
            });
        }

        if let Some(blocks) = &message.content {
            for block in blocks {
                if block.kind.as_deref() == Some("tool_use") {
                    if let Some(name) = &block.name {
                        *stats.tool_calls.entry(name.clone()).or_insert(0) += 1;
                        stats.tool_call_log.push(ToolCall {
                            turn_index,
                            name: name.clone(),
                            file_path: block.input.as_ref().and_then(|i| i.file_path.clone()),
                        });
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
    fn captures_per_turn_timestamp() {
        let file = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-08-01T10:00:00.000Z","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            r#"{"type":"assistant","timestamp":"2026-08-01T10:05:00.000Z","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns[0].timestamp.as_deref(), Some("2026-08-01T10:00:00.000Z"));
        assert_eq!(stats.turns[1].timestamp.as_deref(), Some("2026-08-01T10:05:00.000Z"));
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
    fn logs_tool_calls_with_file_path_and_turn_index() {
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/a/b.rs"}}]}}"#,
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/a/b.rs"}},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.tool_call_log.len(), 3);
        assert_eq!(stats.tool_call_log[0].turn_index, 0);
        assert_eq!(stats.tool_call_log[0].file_path.as_deref(), Some("/a/b.rs"));
        assert_eq!(stats.tool_call_log[1].turn_index, 1);
        // Bash's `input` has no file_path field at all — must not error, just absent.
        assert_eq!(stats.tool_call_log[2].file_path, None);
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
