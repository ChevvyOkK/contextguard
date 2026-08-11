use std::collections::{HashMap, HashSet};
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

/// A block of text that entered the context and is re-sent on every request
/// after it, which is what makes its size worth attributing.
///
/// Sizes are character counts, not tokens: the transcript stores text, and
/// converting is the caller's problem (see `context::approx_tokens`) so the
/// parser stays free of pricing assumptions.
#[derive(Debug, Clone)]
pub struct ContextEntry {
    /// Which turn it first appeared at. Everything from here to the end of
    /// the session is re-sent with it, so an early entry costs far more than
    /// a late one of the same size.
    pub turn_index: usize,
    pub kind: ContextKind,
    pub chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextKind {
    /// A tool's output, named where the call could be matched by id.
    ToolResult(Option<String>),
    /// The arguments the assistant sent to a tool.
    ToolUse(String),
    /// Injected by Claude Code rather than written by either party: hook
    /// output, skill and agent listings, task reminders, memory files. The
    /// string is the attachment's own `type`, which is what makes these
    /// separable at all.
    Attachment(String),
    UserText,
    AssistantText,
    Thinking,
}

#[derive(Debug, Default)]
pub struct SessionStats {
    pub session_id: String,
    pub cwd: Option<String>,
    pub turns: Vec<Turn>,
    pub tool_calls: HashMap<String, u64>,
    pub tool_call_log: Vec<ToolCall>,
    /// Everything that occupies the context window, in the order it arrived.
    pub context_entries: Vec<ContextEntry>,
    /// Prompt tokens on the session's first response — fresh input plus cache
    /// creation plus cache read. The system prompt and the tool schemas are
    /// never written to the transcript, so this total minus the visible
    /// content before it is the only handle we have on their size.
    pub first_prompt_tokens: Option<u64>,
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
    /// Content Claude Code injects on its own account — hook output, skill
    /// and agent listings, task reminders, imported memory files. It occupies
    /// the same context window as the conversation but appears in neither
    /// party's messages, which is why it goes unnoticed.
    #[serde(default)]
    attachment: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    /// The API response id (`msg_…`). Claude Code writes one record per
    /// content block of a response — a turn that thinks and then makes two
    /// tool calls becomes three records — and repeats the whole response's
    /// usage in every one of them. This is what tells them apart from three
    /// genuinely separate responses.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<RawUsage>,
    /// Left as raw JSON on purpose. A user message's content is sometimes a
    /// bare string and sometimes a list of blocks, and a tool_result's
    /// payload is the same story one level down. Typing it as a list made
    /// every string-content line fail to deserialize and get skipped — which
    /// went unnoticed while only assistant records were read.
    #[serde(default)]
    content: Option<serde_json::Value>,
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

pub fn parse_session_file(path: &Path, lang: Lang) -> Result<SessionStats, String> {
    let file = File::open(path).map_err(|e| i18n::err_open_file(lang, &format!("{path:?}"), &e.to_string()))?;
    let reader = BufReader::new(file);

    let session_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
    let mut stats = SessionStats { session_id, ..Default::default() };
    // Response ids already counted, so the extra records a multi-block
    // response produces do not each contribute their copy of the usage.
    let mut seen_responses: HashSet<String> = HashSet::new();
    // tool_use id -> tool name, so a tool_result can be attributed to the
    // tool that produced it. The result block never names it itself.
    let mut tool_names: HashMap<String, String> = HashMap::new();

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

        // Attachments are top-level records with no `message` at all. They
        // are Claude Code talking to itself: hook output, skill listings,
        // task reminders, memory files. They occupy the context window
        // exactly like anything else in it.
        if let Some(attachment) = &raw.attachment {
            let kind = attachment
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            stats.context_entries.push(ContextEntry {
                turn_index: stats.turns.len(),
                kind: ContextKind::Attachment(kind),
                chars: json_text_len(attachment),
            });
            continue;
        }

        let is_assistant = raw.kind.as_deref() == Some("assistant");
        let is_user = raw.kind.as_deref() == Some("user");
        if !is_assistant && !is_user {
            continue;
        }
        let Some(message) = raw.message else { continue };

        // Where content in this record should be attributed. For a record
        // with no usage at all it stays at the next turn boundary, the
        // pre-existing best-effort behaviour.
        let mut turn_index = stats.turns.len();

        if is_assistant {
            if let Some(u) = &message.usage {
                // One API response, several records, the same usage repeated
                // in each: counting them all inflated every figure this tool
                // reports by ~90% on real transcripts. The first record of a
                // response becomes a Turn; the rest are still scanned for
                // their content, they just do not pay twice.
                let first_of_response = match &message.id {
                    Some(id) => seen_responses.insert(id.clone()),
                    // No id to group by: count it rather than silently drop a
                    // real response.
                    None => true,
                };

                if first_of_response {
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
                    if stats.first_prompt_tokens.is_none() {
                        stats.first_prompt_tokens = Some(
                            u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens,
                        );
                    }
                }

                // Every record of a response belongs to the turn that
                // response created.
                turn_index = stats.turns.len().saturating_sub(1);
            }
        }

        let Some(content) = &message.content else { continue };

        // A user message is sometimes just a string rather than a list of
        // blocks. That shape used to fail deserialization outright, taking
        // the whole line with it.
        if let Some(text) = content.as_str() {
            stats.context_entries.push(ContextEntry {
                turn_index,
                kind: ContextKind::UserText,
                chars: text.chars().count(),
            });
            continue;
        }
        let Some(blocks) = content.as_array() else { continue };

        for block in blocks {
            let block_kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match block_kind {
                "tool_use" => {
                    let Some(name) = block.get("name").and_then(|v| v.as_str()) else { continue };
                    *stats.tool_calls.entry(name.to_string()).or_insert(0) += 1;
                    if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                        tool_names.insert(id.to_string(), name.to_string());
                    }
                    let input = block.get("input");
                    stats.tool_call_log.push(ToolCall {
                        turn_index,
                        name: name.to_string(),
                        file_path: input
                            .and_then(|i| i.get("file_path"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    });
                    stats.context_entries.push(ContextEntry {
                        turn_index,
                        kind: ContextKind::ToolUse(name.to_string()),
                        chars: input.map(json_text_len).unwrap_or(0),
                    });
                }
                "tool_result" => {
                    let name = block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .and_then(|id| tool_names.get(id))
                        .cloned();
                    stats.context_entries.push(ContextEntry {
                        turn_index,
                        kind: ContextKind::ToolResult(name),
                        chars: block.get("content").map(json_text_len).unwrap_or(0),
                    });
                }
                "text" => {
                    let chars = block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|t| t.chars().count())
                        .unwrap_or(0);
                    stats.context_entries.push(ContextEntry {
                        turn_index,
                        kind: if is_assistant { ContextKind::AssistantText } else { ContextKind::UserText },
                        chars,
                    });
                }
                "thinking" => {
                    stats.context_entries.push(ContextEntry {
                        turn_index,
                        kind: ContextKind::Thinking,
                        chars: block
                            .get("thinking")
                            .and_then(|v| v.as_str())
                            .map(|t| t.chars().count())
                            .unwrap_or(0),
                    });
                }
                _ => {}
            }
        }
    }

    Ok(stats)
}

/// Length of the human-readable text inside an arbitrary JSON value.
///
/// Tool results and attachments have no single shape: a string, a list of
/// blocks, an object with a `content` or `text` field, or something nested.
/// Summing the string leaves approximates what the model actually reads,
/// where serializing the whole value would also count braces, keys and
/// quoting the model is never billed for.
fn json_text_len(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => s.chars().count(),
        serde_json::Value::Array(items) => items.iter().map(json_text_len).sum(),
        serde_json::Value::Object(map) => map.values().map(json_text_len).sum(),
        // Numbers and booleans are a handful of characters each and never
        // move a total that runs to hundreds of thousands.
        _ => 0,
    }
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
    fn one_response_split_across_records_is_counted_once() {
        // Claude Code writes a record per content block, repeating the whole
        // response's usage in each. Three records, one `msg_` id, one answer.
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":2,"output_tokens":221,"cache_creation_input_tokens":17262,"cache_read_input_tokens":26784},"content":[{"type":"thinking"}]}}"#,
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":2,"output_tokens":221,"cache_creation_input_tokens":17262,"cache_read_input_tokens":26784},"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/a.rs"}}]}}"#,
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":2,"output_tokens":221,"cache_creation_input_tokens":17262,"cache_read_input_tokens":26784},"content":[{"type":"tool_use","name":"Bash"}]}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();

        assert_eq!(stats.turns.len(), 1, "one API response is one turn");
        let total = stats.total_usage();
        assert_eq!(total.output_tokens, 221);
        assert_eq!(total.cache_read_input_tokens, 26_784);
        assert_eq!(total.cache_creation_input_tokens, 17_262);

        // The later records still carry real tool calls; only their copy of
        // the usage is redundant.
        assert_eq!(stats.tool_calls.get("Read"), Some(&1));
        assert_eq!(stats.tool_calls.get("Bash"), Some(&1));
        assert!(
            stats.tool_call_log.iter().all(|c| c.turn_index == 0),
            "every block of a response belongs to that response's turn",
        );
    }

    #[test]
    fn distinct_responses_still_accumulate() {
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            r#"{"type":"assistant","message":{"id":"msg_b","model":"claude-sonnet-5","usage":{"input_tokens":20,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns.len(), 2);
        assert_eq!(stats.total_usage().input_tokens, 30);
    }

    #[test]
    fn responses_without_an_id_are_never_dropped() {
        // Older transcripts, or a truncated write: with nothing to group on,
        // undercounting would be the worse failure.
        let file = write_jsonl(&[
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let stats = parse_session_file(file.path(), Lang::En).unwrap();
        assert_eq!(stats.turns.len(), 2);
        assert_eq!(stats.total_usage().input_tokens, 20);
    }

    #[test]
    fn the_same_id_in_two_sessions_is_two_responses() {
        // Dedup state is per file: ids are only guaranteed unique within a
        // transcript, and sharing a set across files would silently drop
        // real turns from later sessions.
        let a = write_jsonl(&[
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":7,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let b = write_jsonl(&[
            r#"{"type":"assistant","message":{"id":"msg_a","model":"claude-sonnet-5","usage":{"input_tokens":7,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        ]);
        let sa = parse_session_file(a.path(), Lang::En).unwrap();
        let sb = parse_session_file(b.path(), Lang::En).unwrap();
        assert_eq!(sa.total_usage().input_tokens + sb.total_usage().input_tokens, 14);
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
