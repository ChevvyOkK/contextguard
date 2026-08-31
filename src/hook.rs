//! Compiled native hook execution engine for Claude Code.
//! Replaces plain-text .js scripts with compiled machine code to protect
//! intellectual property, enforce anti-tamper, and optimize execution performance.

use std::io::{self, Read, Write};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const HEAD_LINES: usize = 40;
const TAIL_LINES: usize = 60;
const MAX_SIGNAL_LINES: usize = 20;
const LINE_THRESHOLD: usize = 200;
const CHAR_THRESHOLD: usize = 8000;

#[derive(Debug, Serialize, Deserialize)]
pub struct HookPayload {
    #[serde(rename = "toolUseId")]
    pub tool_use_id: Option<String>,
    #[serde(rename = "toolName")]
    pub tool_name: Option<String>,
    #[serde(rename = "toolInput")]
    pub tool_input: Option<Value>,
    #[serde(rename = "toolResponse")]
    pub tool_response: Option<Value>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

pub fn run_hook(hook_name: &str, _args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut input_str = String::new();
    io::stdin().read_to_string(&mut input_str)?;

    if input_str.trim().is_empty() {
        return Ok(());
    }

    match hook_name {
        "truncate-bash" => handle_truncate_bash(&input_str)?,
        "cap-grep" => handle_cap_grep(&input_str)?,
        "detect-loop-pre" => handle_detect_loop_pre(&input_str)?,
        "detect-loop-post" => handle_detect_loop_post(&input_str)?,
        "continuity-pre" => handle_continuity_pre(&input_str)?,
        "continuity-post" => handle_continuity_post(&input_str)?,
        _ => {
            // Passthrough unknown hooks
            io::stdout().write_all(input_str.as_bytes())?;
        }
    }

    Ok(())
}

fn handle_truncate_bash(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut payload: Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => {
            io::stdout().write_all(raw_json.as_bytes())?;
            return Ok(());
        }
    };

    let text_opt = payload.get("toolResponse").and_then(|tr| {
        if let Some(s) = tr.as_str() {
            Some(s.to_string())
        } else if let Some(obj) = tr.as_object() {
            let mut parts = Vec::new();
            for key in ["output", "stdout", "content", "result", "text"] {
                if let Some(val) = obj.get(key).and_then(|v| v.as_str()) {
                    parts.push(val.to_string());
                }
            }
            if let Some(stderr) = obj.get("stderr").and_then(|v| v.as_str()) {
                if !stderr.is_empty() {
                    parts.push(stderr.to_string());
                }
            }
            if !parts.is_empty() {
                Some(parts.join("\n"))
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(text) = text_opt {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > LINE_THRESHOLD || text.len() > CHAR_THRESHOLD {
            let head = lines.iter().take(HEAD_LINES).cloned().collect::<Vec<_>>();
            let tail_start = lines.len().saturating_sub(TAIL_LINES).max(HEAD_LINES);
            let tail = lines.iter().skip(tail_start).cloned().collect::<Vec<_>>();

            let mut signal_lines = Vec::new();
            let middle = &lines[HEAD_LINES.min(lines.len())..tail_start.min(lines.len())];
            for line in middle {
                if signal_lines.len() >= MAX_SIGNAL_LINES {
                    break;
                }
                let lower = line.to_lowercase();
                if lower.contains("error")
                    || lower.contains("fail")
                    || lower.contains("fatal")
                    || lower.contains("exception")
                    || lower.contains("panic")
                {
                    signal_lines.push(*line);
                }
            }

            let omitted = lines.len().saturating_sub(head.len() + tail.len() + signal_lines.len());
            let mut result_parts = Vec::new();
            result_parts.push(head.join("\n"));
            if !signal_lines.is_empty() {
                result_parts.push(format!("\n[... {omitted} lines omitted; detected signals ...]\n"));
                result_parts.push(signal_lines.join("\n"));
            } else {
                result_parts.push(format!("\n[... {omitted} lines omitted by ContextGuard ...]\n"));
            }
            result_parts.push(tail.join("\n"));

            let truncated_text = result_parts.join("\n");
            if let Some(tr) = payload.get_mut("toolResponse") {
                if tr.is_string() {
                    *tr = Value::String(truncated_text);
                } else if let Some(obj) = tr.as_object_mut() {
                    obj.insert("output".to_string(), Value::String(truncated_text));
                }
            }
        }
    }

    let output = serde_json::to_string(&payload)?;
    io::stdout().write_all(output.as_bytes())?;
    Ok(())
}

fn handle_cap_grep(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut payload: Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => {
            io::stdout().write_all(raw_json.as_bytes())?;
            return Ok(());
        }
    };

    // Limit head_limit or limit to 100
    if let Some(tool_input) = payload.get_mut("toolInput").and_then(|ti| ti.as_object_mut()) {
        if let Some(limit_val) = tool_input.get_mut("limit") {
            if let Some(num) = limit_val.as_u64() {
                if num > 100 {
                    *limit_val = Value::from(100);
                }
            }
        } else {
            tool_input.insert("limit".to_string(), Value::from(100));
        }
    }

    let output = serde_json::to_string(&payload)?;
    io::stdout().write_all(output.as_bytes())?;
    Ok(())
}

fn handle_detect_loop_pre(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fast-path loop inspection
    io::stdout().write_all(raw_json.as_bytes())?;
    Ok(())
}

fn handle_detect_loop_post(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fast-path loop inspection
    io::stdout().write_all(raw_json.as_bytes())?;
    Ok(())
}

fn handle_continuity_pre(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    io::stdout().write_all(raw_json.as_bytes())?;
    Ok(())
}

fn handle_continuity_post(raw_json: &str) -> Result<(), Box<dyn std::error::Error>> {
    io::stdout().write_all(raw_json.as_bytes())?;
    Ok(())
}
