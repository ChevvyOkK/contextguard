use std::path::PathBuf;

use crate::i18n::{self, Lang};

/// Claude Code stores one JSONL transcript per session under
/// `~/.claude/projects/<escaped-cwd>/<session-id>.jsonl`. This walks that
/// tree (non-recursively into project dirs, since each is flat) and returns
/// every `.jsonl` file found, newest first.
pub fn find_session_files(within_days: Option<u64>, lang: Lang) -> Result<Vec<PathBuf>, String> {
    let home = dirs::home_dir().ok_or_else(|| i18n::err_home_dir_not_found(lang).to_string())?;
    let projects_dir = home.join(".claude").join("projects");

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let cutoff = within_days.map(|days| {
        std::time::SystemTime::now() - std::time::Duration::from_secs(days * 24 * 60 * 60)
    });

    for project_entry in std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())?.flatten() {
        if !project_entry.path().is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(project_entry.path()) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else { continue };
            let Ok(modified) = metadata.modified() else { continue };
            if let Some(cutoff) = cutoff {
                if modified < cutoff {
                    continue;
                }
            }
            files.push((modified, path));
        }
    }

    files.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(files.into_iter().map(|(_, p)| p).collect())
}
