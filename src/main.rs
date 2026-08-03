mod claude_md;
mod discovery;
mod i18n;
mod pricing;
mod report;
mod savings;
mod session;

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;

use i18n::Lang;

/// ContextGuard — local audit of Claude Code token spend. Nothing is sent
/// anywhere: it only reads session files already on this machine.
#[derive(Parser, Debug)]
#[command(name = "contextguard", version, about)]
struct Cli {
    /// Only consider sessions from the last N days (default: all)
    #[arg(long)]
    days: Option<u64>,

    /// Path to a CLAUDE.md to analyze (default: looked up in the current directory)
    #[arg(long)]
    claude_md: Option<PathBuf>,

    /// Output language: "en" or "ru" (default: en, or $CONTEXTGUARD_LANG)
    #[arg(long)]
    lang: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let lang = Lang::detect(cli.lang.as_deref());

    let files = match discovery::find_session_files(cli.days, lang) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("{}", i18n::err_finding_session_files(lang, &e).red());
            std::process::exit(1);
        }
    };

    if files.is_empty() {
        println!("{}", i18n::no_session_files_found(lang).yellow());
        return;
    }

    let mut sessions = Vec::with_capacity(files.len());
    for file in &files {
        match session::parse_session_file(file, lang) {
            Ok(stats) => sessions.push(stats),
            Err(e) => eprintln!("{}", i18n::skip_file(lang, &format!("{file:?}"), &e).dimmed()),
        }
    }

    let pricing = pricing::PricingTable::defaults();
    let agg = report::aggregate(&sessions, &pricing);

    let claude_md_path = cli.claude_md.or_else(|| {
        let candidate = std::env::current_dir().ok()?.join("CLAUDE.md");
        candidate.exists().then_some(candidate)
    });
    let claude_md_report = claude_md_path.as_deref().and_then(|p| claude_md::analyze(p, lang).ok());
    let savings_report = savings::read();

    report::print_report(&agg, claude_md_report.as_ref(), &savings_report, lang);
}
