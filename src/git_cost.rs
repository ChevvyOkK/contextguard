//! Phase 7 — attributes session cost to git branches and merged PRs, by
//! time. Nothing here talks to `git` directly (see `gitlog.rs` for that);
//! this module is pure computation over a `Commit` list and the already-
//! parsed `SessionStats`, which is what makes it unit-testable without a
//! real repository.
//!
//! ## The core assumption, stated plainly
//!
//! Claude Code transcripts carry no record of which git branch was checked
//! out at the time. The only signal available is *when* — a session's turn
//! timestamps against a branch's commit timestamps. So: a branch's "active
//! window" is `[first_commit_time − lookback, last_commit_time]`, and every
//! session turn whose timestamp falls inside that window is attributed to
//! it. `lookback` exists because the work that produces a commit happens
//! *before* the commit, not at the instant of `git commit`.
//!
//! This is a heuristic, not a trace, and it has two known failure modes,
//! both left as-is rather than papered over:
//! - Switching between branches inside the same lookback window can double-
//!   count a turn against more than one branch.
//! - Work that never gets committed (an abandoned attempt, exploration)
//!   attributes to whatever window it happened to fall in, or nothing at
//!   all if it's outside every window.
//!
//! Good enough to answer "roughly what did this feature cost", not
//! precise enough to reconcile a bill — the same honesty bar the rest of
//! this tool holds itself to.

use crate::gitlog::{self, Commit, MergeFilter};
use crate::pricing::PricingTable;
use crate::session::SessionStats;
use crate::timeutil::parse_epoch_seconds;

pub enum ItemKind {
    /// A branch that still exists locally, not yet merged (or merged but
    /// not yet deleted).
    Branch,
    /// Recovered from a merge commit (regular merge) or a squash-merge
    /// commit's message on the base branch — the source branch may be long
    /// gone.
    MergedPr,
}

pub struct CostItem {
    pub label: String,
    pub kind: ItemKind,
    pub commit_count: usize,
    pub first_commit_epoch: i64,
    /// Human-readable local date of the earliest commit, straight from
    /// `git log`'s own formatting — shown as-is rather than reformatted, so
    /// what's on screen always matches what `git log` itself would show for
    /// that commit.
    pub first_commit_date_display: String,
    pub last_commit_epoch: i64,
    pub cost_usd: f64,
    pub turns_counted: usize,
    /// True when this item's commit history is a single squash-merge commit
    /// with no recoverable sub-commits — the window is `lookback`-wide
    /// ending at that one commit, not bounded by real first/last commits of
    /// the original work.
    pub squashed: bool,
}

pub struct GitCostReport {
    pub base_branch: String,
    pub lookback_secs: i64,
    pub items: Vec<CostItem>,
    /// True if turns existed for this repo but none fell inside any item's
    /// window — worth saying explicitly rather than just showing an empty
    /// list indistinguishable from "no local Claude Code sessions at all".
    pub repo_turns_found: usize,
}

/// Extracts a PR number from a commit subject, matching the two shapes
/// GitHub itself produces by default:
/// - a regular merge commit: `"Merge pull request #123 from owner/branch"`
/// - a squash merge: `"<PR title> (#123)"`
///
/// Purely local string matching — no network call, no `gh` CLI dependency.
pub fn find_pr_number(subject: &str) -> Option<u32> {
    const MARKER: &str = "pull request #";
    if let Some(idx) = subject.find(MARKER) {
        let rest = &subject[idx + MARKER.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }

    let trimmed = subject.trim_end();
    if let Some(rest) = trimmed.strip_suffix(')') {
        if let Some(open) = rest.rfind('(') {
            if let Some(digits) = rest[open + 1..].strip_prefix('#') {
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    if let Ok(n) = digits.parse() {
                        return Some(n);
                    }
                }
            }
        }
    }

    None
}

struct CostTurn {
    epoch: f64,
    cost_usd: f64,
}

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").to_ascii_lowercase()
}

/// Every priced turn from sessions whose `cwd` is inside `repo_root` —
/// including subdirectories, since a session can run from anywhere under
/// the repo, not just its top level.
fn collect_repo_turns(sessions: &[SessionStats], pricing: &PricingTable, repo_root: &str) -> Vec<CostTurn> {
    let root_norm = normalize_path(repo_root);
    let mut turns = Vec::new();

    for session in sessions {
        let Some(cwd) = &session.cwd else { continue };
        if !normalize_path(cwd).starts_with(&root_norm) {
            continue;
        }
        for turn in &session.turns {
            let Some(ts) = &turn.timestamp else { continue };
            let Some(epoch) = parse_epoch_seconds(ts) else { continue };
            turns.push(CostTurn { epoch, cost_usd: pricing.cost_usd(&turn.model, &turn.usage) });
        }
    }

    turns.sort_by(|a, b| a.epoch.partial_cmp(&b.epoch).unwrap());
    turns
}

fn sum_in_window(turns: &[CostTurn], start_epoch: f64, end_epoch: f64) -> (f64, usize) {
    let mut cost = 0.0;
    let mut count = 0;
    for t in turns {
        if t.epoch >= start_epoch && t.epoch <= end_epoch {
            cost += t.cost_usd;
            count += 1;
        }
    }
    (cost, count)
}

fn build_item(label: String, kind: ItemKind, commits: &[Commit], turns: &[CostTurn], lookback_secs: i64, squashed: bool) -> Option<CostItem> {
    let earliest = commits.iter().min_by_key(|c| c.epoch)?;
    let first_commit_epoch = earliest.epoch;
    let first_commit_date_display = earliest.date_display.clone();
    let last_commit_epoch = commits.iter().map(|c| c.epoch).max().unwrap();
    let window_start = (first_commit_epoch - lookback_secs) as f64;
    let window_end = last_commit_epoch as f64;
    let (cost_usd, turns_counted) = sum_in_window(turns, window_start, window_end);

    Some(CostItem {
        label,
        kind,
        commit_count: commits.len(),
        first_commit_epoch,
        first_commit_date_display,
        last_commit_epoch,
        cost_usd,
        turns_counted,
        squashed,
    })
}

pub struct BuildOptions<'a> {
    pub base_branch: Option<&'a str>,
    pub since_days: Option<u64>,
    pub lookback_secs: i64,
}

/// Walks the repository containing `start_dir` and attributes local session
/// cost to (a) every local branch that has diverged from the base branch,
/// and (b) every merged PR (regular-merge or squash) found on the base
/// branch itself. Errors only for reasons a user needs to act on: not a git
/// repo, or no determinable base branch.
pub fn build_report(
    start_dir: &std::path::Path,
    sessions: &[SessionStats],
    pricing: &PricingTable,
    opts: &BuildOptions,
) -> Result<GitCostReport, String> {
    let root = gitlog::repo_root(start_dir)?;
    let base_branch = match opts.base_branch {
        Some(b) => b.to_string(),
        None => gitlog::default_branch(&root)?,
    };
    gitlog::verify_revision(&root, &base_branch)
        .map_err(|_| format!("base branch '{base_branch}' does not exist in this repository"))?;

    let repo_root_str = root.to_string_lossy().to_string();
    let turns = collect_repo_turns(sessions, pricing, &repo_root_str);

    let mut items = Vec::new();

    for branch in gitlog::local_branches(&root)? {
        if branch == base_branch {
            continue;
        }
        let Ok(base_point) = gitlog::merge_base(&root, &base_branch, &branch) else { continue };
        let range = format!("{base_point}..{branch}");
        let Ok(commits) = gitlog::log(&root, &range, None, MergeFilter::Any) else { continue };
        if let Some(item) = build_item(branch, ItemKind::Branch, &commits, &turns, opts.lookback_secs, false) {
            items.push(item);
        }
    }

    let since = opts.since_days.map(|d| format!("{d} days ago"));

    if let Ok(merges) = gitlog::log(&root, &base_branch, since.as_deref(), MergeFilter::MergesOnly) {
        for mc in &merges {
            let Some(pr_number) = find_pr_number(&mc.subject) else { continue };
            // A regular (2-parent) merge commit's PR history is exactly the
            // commits unique to its second parent relative to its first —
            // the same range the branch would have had before merging.
            let [p1, p2] = match mc.parents.as_slice() {
                [p1, p2] => [p1.clone(), p2.clone()],
                _ => continue,
            };
            let range = format!("{p1}..{p2}");
            let Ok(commits) = gitlog::log(&root, &range, None, MergeFilter::Any) else { continue };
            if let Some(item) = build_item(format!("PR #{pr_number}"), ItemKind::MergedPr, &commits, &turns, opts.lookback_secs, false) {
                items.push(item);
            }
        }
    }

    if let Ok(non_merges) = gitlog::log(&root, &base_branch, since.as_deref(), MergeFilter::NoMerges) {
        for commit in &non_merges {
            let Some(pr_number) = find_pr_number(&commit.subject) else { continue };
            // A squash merge leaves no separate branch history — this one
            // commit is the entire recoverable record of that PR's work.
            let single = std::slice::from_ref(commit);
            if let Some(item) = build_item(format!("PR #{pr_number}"), ItemKind::MergedPr, single, &turns, opts.lookback_secs, true) {
                items.push(item);
            }
        }
    }

    items.sort_by(|a, b| b.cost_usd.partial_cmp(&a.cost_usd).unwrap());

    Ok(GitCostReport {
        base_branch,
        lookback_secs: opts.lookback_secs,
        items,
        repo_turns_found: turns.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Turn, Usage};

    fn commit(epoch: i64, subject: &str, parents: &[&str]) -> Commit {
        Commit {
            parents: parents.iter().map(|p| p.to_string()).collect(),
            epoch,
            date_display: String::new(),
            subject: subject.to_string(),
        }
    }

    fn session_with_turn(cwd: &str, ts: &str, cost_input_tokens: u64) -> SessionStats {
        SessionStats {
            session_id: "s".into(),
            cwd: Some(cwd.to_string()),
            turns: vec![Turn {
                model: "claude-sonnet-5".into(),
                usage: Usage { input_tokens: cost_input_tokens, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 },
                timestamp: Some(ts.to_string()),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn find_pr_number_matches_merge_commit_subject() {
        assert_eq!(find_pr_number("Merge pull request #212 from me/feat-x"), Some(212));
    }

    #[test]
    fn find_pr_number_matches_squash_merge_subject() {
        assert_eq!(find_pr_number("Add checkout redesign (#45)"), Some(45));
    }

    #[test]
    fn find_pr_number_ignores_unrelated_parens() {
        assert_eq!(find_pr_number("fix: handle (edge case) properly"), None);
    }

    #[test]
    fn find_pr_number_ignores_plain_commits() {
        assert_eq!(find_pr_number("fix: auth race condition"), None);
    }

    #[test]
    fn sums_only_turns_inside_the_commit_window() {
        let turns = vec![
            CostTurn { epoch: 1000.0, cost_usd: 1.0 }, // before window
            CostTurn { epoch: 1500.0, cost_usd: 2.0 }, // inside
            CostTurn { epoch: 1800.0, cost_usd: 3.0 }, // inside
            CostTurn { epoch: 2500.0, cost_usd: 4.0 }, // after window
        ];
        let (cost, count) = sum_in_window(&turns, 1400.0, 2000.0);
        assert!((cost - 5.0).abs() < 1e-9);
        assert_eq!(count, 2);
    }

    #[test]
    fn build_item_applies_lookback_before_the_first_commit() {
        let commits = vec![commit(10_000, "wip", &["p"])];
        let turns = vec![
            CostTurn { epoch: 9_000.0, cost_usd: 5.0 },  // 1000s before commit, inside a 2000s lookback
            CostTurn { epoch: 7_000.0, cost_usd: 99.0 }, // outside lookback
        ];
        let item = build_item("feat/x".into(), ItemKind::Branch, &commits, &turns, 2000, false).unwrap();
        assert!((item.cost_usd - 5.0).abs() < 1e-9);
        assert_eq!(item.turns_counted, 1);
    }

    #[test]
    fn build_item_returns_none_for_empty_commits() {
        assert!(build_item("x".into(), ItemKind::Branch, &[], &[], 100, false).is_none());
    }

    #[test]
    fn build_item_uses_the_earliest_commits_date_display() {
        let mut early = commit(100, "wip", &["p"]);
        early.date_display = "2026-08-01 09:00:00 +0000".to_string();
        let mut late = commit(200, "more", &["p"]);
        late.date_display = "2026-08-01 11:00:00 +0000".to_string();
        // Passed out of chronological order on purpose — build_item must
        // find the earliest by epoch, not assume the caller already sorted.
        let item = build_item("x".into(), ItemKind::Branch, &[late, early], &[], 100, false).unwrap();
        assert_eq!(item.first_commit_date_display, "2026-08-01 09:00:00 +0000");
    }

    #[test]
    fn collect_repo_turns_filters_by_cwd_prefix() {
        let sessions = vec![
            session_with_turn("/home/me/contextguard", "2026-08-01T10:00:00.000Z", 1_000_000),
            session_with_turn("/home/me/contextguard/src", "2026-08-01T10:05:00.000Z", 1_000_000),
            session_with_turn("/home/me/other-repo", "2026-08-01T10:10:00.000Z", 1_000_000),
        ];
        let pricing = PricingTable::defaults();
        let turns = collect_repo_turns(&sessions, &pricing, "/home/me/contextguard");
        assert_eq!(turns.len(), 2, "the subdirectory session must still match, the sibling repo must not");
    }

    #[test]
    fn collect_repo_turns_is_case_and_slash_insensitive_for_windows_paths() {
        let sessions = vec![session_with_turn(r"C:\lol\ContextGuard\contextguard\src", "2026-08-01T10:00:00.000Z", 1_000_000)];
        let pricing = PricingTable::defaults();
        let turns = collect_repo_turns(&sessions, &pricing, r"c:\lol\contextguard\contextguard");
        assert_eq!(turns.len(), 1);
    }

    #[test]
    fn items_are_sorted_by_cost_descending() {
        // Turns must land *before* their commit (work happens, then you
        // commit) — the window is [commit_epoch - lookback, commit_epoch].
        let turns = vec![CostTurn { epoch: 100.0, cost_usd: 1.0 }, CostTurn { epoch: 200.0, cost_usd: 50.0 }];
        let cheap = build_item("cheap".into(), ItemKind::Branch, &[commit(105, "a", &["p"])], &turns, 10, false).unwrap();
        let pricey = build_item("pricey".into(), ItemKind::Branch, &[commit(205, "b", &["p"])], &turns, 10, false).unwrap();
        let mut items = [cheap, pricey];
        items.sort_by(|a, b| b.cost_usd.partial_cmp(&a.cost_usd).unwrap());
        assert_eq!(items[0].label, "pricey");
    }
}
