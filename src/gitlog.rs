//! Thin shell-out wrapper around the `git` CLI. Kept separate from
//! `git_cost.rs` so the attribution logic itself stays unit-testable on
//! synthetic commit lists, with no real repository or `git` binary required
//! to run those tests.
//!
//! Every commit's time is read as a Unix timestamp (`%at`) rather than an
//! ISO 8601 string, sidestepping timezone-offset parsing entirely — the
//! project's existing `timeutil::parse_epoch_seconds` only understands a
//! trailing `Z` (UTC), which is what Claude Code's own transcripts use, but
//! `git log`'s default ISO format uses the commit author's local offset.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Commit {
    /// Full hashes of the commit's parents — empty for a root commit, one
    /// for an ordinary commit, two (or more, for an octopus merge) for a
    /// merge commit. This is what tells a merge commit apart from an
    /// ordinary one without re-parsing the subject line.
    pub parents: Vec<String>,
    pub epoch: i64,
    /// For display only — human-readable local date, not used in any
    /// comparison (that's what `epoch` is for).
    pub date_display: String,
    pub subject: String,
}

fn run_git(repo_root: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The top-level directory of the git repository containing `start_dir`, or
/// an error if `start_dir` isn't inside a git working tree at all — the
/// caller treats that as "nothing to attribute", not a crash.
pub fn repo_root(start_dir: &std::path::Path) -> Result<PathBuf, String> {
    let out = run_git(start_dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out))
}

/// Best-effort default branch: the remote's HEAD if one is configured, else
/// whichever of `main`/`master` exists locally. Anything more clever (e.g.
/// asking a code host's API) would need network access this tool doesn't
/// otherwise use — a repo that uses neither name and has no remote HEAD set
/// needs `--base` supplied explicitly.
pub fn default_branch(repo_root: &std::path::Path) -> Result<String, String> {
    if let Ok(sym) = run_git(repo_root, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        if let Some(name) = sym.rsplit('/').next() {
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }
    for candidate in ["main", "master"] {
        if run_git(repo_root, &["rev-parse", "--verify", "--quiet", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Err("could not determine the default branch (no origin/HEAD, no local main or master) — pass --base explicitly".to_string())
}

/// Local branch names (`refs/heads/*`), not remote-tracking branches — a
/// remote branch nobody has checked out locally has no local commits to
/// attribute anything to.
pub fn local_branches(repo_root: &std::path::Path) -> Result<Vec<String>, String> {
    let out = run_git(repo_root, &["branch", "--format=%(refname:short)"])?;
    Ok(out.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
}

pub fn merge_base(repo_root: &std::path::Path, a: &str, b: &str) -> Result<String, String> {
    run_git(repo_root, &["merge-base", a, b])
}

/// Confirms `revision` actually resolves to a commit in this repo. Used to
/// fail loudly on a bad `--base` rather than have every subsequent git call
/// against it quietly error out one by one, leaving nothing but a
/// misleading empty report.
pub fn verify_revision(repo_root: &std::path::Path, revision: &str) -> Result<(), String> {
    run_git(repo_root, &["rev-parse", "--verify", "--quiet", revision]).map(|_| ())
}

#[derive(Clone, Copy)]
pub enum MergeFilter {
    Any,
    MergesOnly,
    NoMerges,
}

const FIELD_SEP: &str = "\x1f";
const RECORD_SEP: &str = "\x1e";

/// Commits reachable from `revision` — a plain ref (`"main"`) walks that
/// branch's whole history, a range (`"base..tip"`) walks only what's unique
/// to `tip` — oldest first, the order the attribution logic wants for
/// "first commit" / "last commit" bookkeeping.
///
/// `since` bounds how far back to look (git's own `--since`, e.g.
/// `"30 days ago"`); `None` means unbounded, matching how
/// `discovery::find_session_files` treats a missing `--days`.
pub fn log(
    repo_root: &std::path::Path,
    revision: &str,
    since: Option<&str>,
    filter: MergeFilter,
) -> Result<Vec<Commit>, String> {
    let format = format!("--pretty=format:%P{FIELD_SEP}%at{FIELD_SEP}%ai{FIELD_SEP}%s{RECORD_SEP}");
    let mut args: Vec<String> = vec!["log".into(), "--reverse".into(), format];
    match filter {
        MergeFilter::Any => {}
        MergeFilter::MergesOnly => args.push("--merges".into()),
        MergeFilter::NoMerges => args.push("--no-merges".into()),
    }
    if let Some(s) = since {
        args.push(format!("--since={s}"));
    }
    args.push(revision.to_string());

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(repo_root, &args_ref)?;
    Ok(parse_log_output(&out))
}

fn parse_log_output(out: &str) -> Vec<Commit> {
    out.split(RECORD_SEP)
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(4, FIELD_SEP);
            let parents = fields.next()?.split_whitespace().map(str::to_string).collect();
            let epoch = fields.next()?.parse().ok()?;
            let date_display = fields.next()?.to_string();
            let subject = fields.next().unwrap_or("").to_string();
            Some(Commit { parents, epoch, date_display, subject })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_commit_record() {
        let raw = format!("def456{FIELD_SEP}1700000000{FIELD_SEP}2023-11-14 12:00:00 +0000{FIELD_SEP}fix: thing{RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].parents, vec!["def456".to_string()]);
        assert_eq!(commits[0].epoch, 1_700_000_000);
        assert_eq!(commits[0].subject, "fix: thing");
    }

    #[test]
    fn parses_a_root_commit_with_no_parents() {
        let raw = format!("{FIELD_SEP}1700000000{FIELD_SEP}2023-11-14 12:00:00 +0000{FIELD_SEP}init{RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits.len(), 1);
        assert!(commits[0].parents.is_empty());
    }

    #[test]
    fn parses_a_merge_commit_with_two_parents() {
        let raw = format!("p1 p2{FIELD_SEP}1700000000{FIELD_SEP}2023-11-14 12:00:00 +0000{FIELD_SEP}Merge pull request #7{RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits[0].parents, vec!["p1".to_string(), "p2".to_string()]);
    }

    #[test]
    fn parses_multiple_records_in_order() {
        let raw = format!("{FIELD_SEP}1{FIELD_SEP}d1{FIELD_SEP}first{RECORD_SEP}a{FIELD_SEP}2{FIELD_SEP}d2{FIELD_SEP}second{RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "first");
        assert_eq!(commits[1].subject, "second");
    }

    #[test]
    fn ignores_empty_and_whitespace_only_records() {
        let raw = format!("  {RECORD_SEP}{RECORD_SEP}{FIELD_SEP}1{FIELD_SEP}d{FIELD_SEP}s{RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits.len(), 1);
    }

    #[test]
    fn a_subject_containing_colons_and_parens_is_kept_whole() {
        let raw = format!("{FIELD_SEP}1{FIELD_SEP}d{FIELD_SEP}feat: a, b: c (#9){RECORD_SEP}");
        let commits = parse_log_output(&raw);
        assert_eq!(commits[0].subject, "feat: a, b: c (#9)");
    }
}
