use std::path::Path;

pub const RECOMMENDED_MAX_LINES: usize = 200;

/// Generic phrases that restate things the model already knows how to do
/// and add no project-specific information — the exact kind of line that
/// costs tokens on every single session without earning its keep.
const GENERIC_BOILERPLATE: &[&str] = &[
    "write clean code",
    "follow best practices",
    "be helpful",
    "write good code",
    "use meaningful variable names",
    "add comments to explain",
    "write readable code",
    "follow the dry principle",
    "keep it simple",
    "write maintainable code",
];

#[derive(Debug)]
pub struct ClaudeMdReport {
    pub path: String,
    pub line_count: usize,
    pub approx_tokens: usize,
    pub generic_lines: Vec<(usize, String)>,
    pub over_recommended: bool,
}

/// Rough chars-per-token estimate for English/code text; good enough for
/// spotting a bloated file, not for billing precision.
fn approx_token_count(text: &str) -> usize {
    (text.chars().count() as f64 / 4.0).ceil() as usize
}

pub fn analyze(path: &Path) -> Result<ClaudeMdReport, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("не удалось прочитать {path:?}: {e}"))?;

    analyze_content(&path.display().to_string(), &content)
}

fn analyze_content(path_label: &str, content: &str) -> Result<ClaudeMdReport, String> {
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    let mut generic_lines = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        for phrase in GENERIC_BOILERPLATE {
            if lower.contains(phrase) {
                generic_lines.push((i + 1, line.trim().to_string()));
                break;
            }
        }
    }

    Ok(ClaudeMdReport {
        path: path_label.to_string(),
        line_count,
        approx_tokens: approx_token_count(content),
        generic_lines,
        over_recommended: line_count > RECOMMENDED_MAX_LINES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_file_over_recommended_length() {
        let content = "line\n".repeat(250);
        let report = analyze_content("test", &content).unwrap();
        assert!(report.over_recommended);
        assert_eq!(report.line_count, 250);
    }

    #[test]
    fn does_not_flag_file_under_recommended_length() {
        let content = "line\n".repeat(50);
        let report = analyze_content("test", &content).unwrap();
        assert!(!report.over_recommended);
    }

    #[test]
    fn detects_generic_boilerplate_lines() {
        let content = "Project: foo\nAlways write clean code.\nUse Postgres for storage.\n";
        let report = analyze_content("test", content).unwrap();
        assert_eq!(report.generic_lines.len(), 1);
        assert_eq!(report.generic_lines[0].0, 2);
    }

    #[test]
    fn boilerplate_detection_is_case_insensitive() {
        let content = "WRITE CLEAN CODE always\n";
        let report = analyze_content("test", content).unwrap();
        assert_eq!(report.generic_lines.len(), 1);
    }

    #[test]
    fn approx_token_count_scales_with_length() {
        let short = analyze_content("test", "hi").unwrap();
        let long = analyze_content("test", &"a".repeat(4000)).unwrap();
        assert!(long.approx_tokens > short.approx_tokens * 100);
    }
}
