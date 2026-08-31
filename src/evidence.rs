use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct EvidenceEvent {
    pub id: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(default, rename = "type")]
    pub event_type: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default, rename = "exactImpact")]
    pub exact_impact: Option<Value>,
    #[serde(default, rename = "estimatedImpact")]
    pub estimated_impact: Option<Value>,
    #[serde(default, rename = "localReferences")]
    pub local_references: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallResult {
    EventJson(String),
    VaultOutput {
        id: String,
        path: PathBuf,
        content: String,
    },
}

fn contextguard_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".claude").join("contextguard"))
}

pub fn evidence_path() -> Option<PathBuf> {
    Some(contextguard_dir()?.join("evidence").join("events.jsonl"))
}

pub fn vault_dir() -> Option<PathBuf> {
    Some(contextguard_dir()?.join("vault"))
}

pub fn read_events(limit: usize) -> Vec<EvidenceEvent> {
    let Some(path) = evidence_path() else {
        return Vec::new();
    };
    read_events_from_path(&path, limit)
}

pub fn read_events_from_path(path: &Path, limit: usize) -> Vec<EvidenceEvent> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<EvidenceEvent>(&line) else {
            continue;
        };
        events.push(event);
    }

    if limit == 0 || events.len() <= limit {
        return events;
    }

    events.split_off(events.len() - limit)
}

pub fn recall(id: &str) -> Result<RecallResult, String> {
    let Some(events_path) = evidence_path() else {
        return Err("home directory not found".to_string());
    };
    let Some(vault_dir) = vault_dir() else {
        return Err("home directory not found".to_string());
    };
    recall_from_paths(id, &events_path, &vault_dir)
}

pub fn recall_from_paths(
    id: &str,
    events_path: &Path,
    vault_dir: &Path,
) -> Result<RecallResult, String> {
    let clean = sanitize_id(id);
    if clean.is_empty() {
        return Err("empty event or vault id".to_string());
    }

    if let Some(result) = recall_vault_id(&clean, vault_dir) {
        return Ok(result);
    }

    let events = read_events_from_path(events_path, 0);
    let Some(event) = events.into_iter().find(|event| event.id == clean) else {
        return Err(format!(
            "no evidence event or vault output found for {clean}"
        ));
    };

    if let Some(vault_id) = event.local_references.as_ref().and_then(vault_id_from_refs) {
        if let Some(result) = recall_vault_id(&vault_id, vault_dir) {
            return Ok(result);
        }
    }

    serde_json::to_string_pretty(&event_to_value(&event))
        .map(RecallResult::EventJson)
        .map_err(|e| e.to_string())
}

fn recall_vault_id(id: &str, vault_dir: &Path) -> Option<RecallResult> {
    if !id.starts_with("CG-") {
        return None;
    }
    let path = vault_dir.join(format!("{id}.log"));
    let Ok(content) = std::fs::read_to_string(&path) else {
        return None;
    };
    Some(RecallResult::VaultOutput {
        id: id.to_string(),
        path,
        content,
    })
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect()
}

fn vault_id_from_refs(value: &Value) -> Option<String> {
    value
        .get("vaultId")
        .and_then(Value::as_str)
        .map(sanitize_id)
        .filter(|id| id.starts_with("CG-"))
}

fn event_to_value(event: &EvidenceEvent) -> Value {
    serde_json::json!({
        "id": event.id,
        "timestamp": event.timestamp,
        "project": event.project,
        "sessionId": event.session_id,
        "type": event.event_type,
        "severity": event.severity,
        "confidence": event.confidence,
        "evidence": event.evidence,
        "action": event.action,
        "exactImpact": event.exact_impact,
        "estimatedImpact": event.estimated_impact,
        "localReferences": event.local_references,
    })
}

pub fn value_summary(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut parts: Vec<String> = map
                .iter()
                .take(6)
                .map(|(key, value)| format!("{key}={}", scalar_summary(value)))
                .collect();
            if map.len() > parts.len() {
                parts.push("...".to_string());
            }
            parts.join(", ")
        }
        _ => scalar_summary(value),
    }
}

fn scalar_summary(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Array(v) => format!("[{} item(s)]", v.len()),
        Value::Object(v) => format!("{{{} field(s)}}", v.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_recent_events_and_ignores_malformed_lines() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "not json").unwrap();
        writeln!(
            file,
            r#"{{"id":"CGE-1","timestamp":"2026-08-23T00:00:00Z","type":"OUTPUT_TRUNCATED","evidence":["stored"],"exactImpact":{{"originalChars":100}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"id":"CGE-2","timestamp":"2026-08-23T00:01:00Z","type":"NO_PROGRESS_DETECTED","evidence":["same failure"]}}"#
        )
        .unwrap();

        let events = read_events_from_path(file.path(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "CGE-2");
        assert_eq!(
            events[0].event_type.as_deref(),
            Some("NO_PROGRESS_DETECTED")
        );
    }

    #[test]
    fn recalls_vault_output_by_vault_id() {
        let temp = tempfile::tempdir().unwrap();
        let vault_dir = temp.path().join("vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        std::fs::write(vault_dir.join("CG-ABC.log"), "full output").unwrap();
        let events = temp.path().join("events.jsonl");
        std::fs::write(&events, "").unwrap();

        let result = recall_from_paths("CG-ABC", &events, &vault_dir).unwrap();
        match result {
            RecallResult::VaultOutput { id, content, .. } => {
                assert_eq!(id, "CG-ABC");
                assert_eq!(content, "full output");
            }
            RecallResult::EventJson(_) => panic!("expected vault output"),
        }
    }

    #[test]
    fn recalls_vault_output_by_evidence_event_id() {
        let temp = tempfile::tempdir().unwrap();
        let vault_dir = temp.path().join("vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        std::fs::write(vault_dir.join("CG-ABC.log"), "full output").unwrap();
        let events = temp.path().join("events.jsonl");
        std::fs::write(
            &events,
            r#"{"id":"CGE-1","type":"OUTPUT_TRUNCATED","localReferences":{"vaultId":"CG-ABC"}}"#,
        )
        .unwrap();

        let result = recall_from_paths("CGE-1", &events, &vault_dir).unwrap();
        assert!(matches!(result, RecallResult::VaultOutput { .. }));
    }

    #[test]
    fn recalls_non_vault_event_as_pretty_json() {
        let temp = tempfile::tempdir().unwrap();
        let vault_dir = temp.path().join("vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        let events = temp.path().join("events.jsonl");
        std::fs::write(&events, r#"{"id":"CGE-1","type":"NO_PROGRESS_DETECTED"}"#).unwrap();

        let result = recall_from_paths("CGE-1", &events, &vault_dir).unwrap();
        match result {
            RecallResult::EventJson(json) => assert!(json.contains("NO_PROGRESS_DETECTED")),
            RecallResult::VaultOutput { .. } => panic!("expected event json"),
        }
    }
}
