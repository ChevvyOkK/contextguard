mod claude_md;
mod context;
mod discovery;
mod i18n;
mod optimize;
mod pricing;
mod push;
mod report;
mod savings;
mod session;
mod timeutil;

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;

use i18n::Lang;

/// Text: the full interactive report. Markdown: a compact summary + top-3
/// cross-algorithm findings, meant for pasting into Slack or a GitHub PR.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Markdown,
}

/// Output format for `contextguard context`, which has a machine-readable
/// mode the headline report does not: the breakdown is the part people want
/// to feed into something else.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ContextFormat {
    Text,
    Json,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Break down what is occupying the context window, and what re-sending
    /// it costs.
    Context {
        /// "text" (default) or "json"
        #[arg(long, value_enum, default_value_t = ContextFormat::Text)]
        format: ContextFormat,
    },
}

/// ContextGuard — local audit of Claude Code token spend. Nothing is sent
/// anywhere unless you explicitly pass --push: it only reads session files
/// already on this machine.
#[derive(Parser, Debug)]
#[command(name = "contextguard", version, about)]
struct Cli {
    /// Run a specific analysis. Omit for the headline report.
    #[command(subcommand)]
    command: Option<Command>,

    /// Only consider sessions from the last N days (default: all)
    #[arg(long, global = true)]
    days: Option<u64>,

    /// Path to a CLAUDE.md to analyze (default: looked up in the current directory)
    #[arg(long, global = true)]
    claude_md: Option<PathBuf>,

    /// Output format: "text" (default, full report) or "markdown" (compact,
    /// top-3 findings — for Slack/GitHub PR). Deliberately not global: the
    /// context subcommand has its own --format with a different set of
    /// values, and two globals with one name collide.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Output language: "en" or "ru" (default: en, or $CONTEXTGUARD_LANG)
    #[arg(long, global = true)]
    lang: Option<String>,

    /// Push aggregated daily snapshots (numbers only, no code/content) to the dashboard
    #[arg(long)]
    push: bool,

    /// Dashboard API base URL for --push (default: $CONTEXTGUARD_API_URL)
    #[arg(long)]
    api_url: Option<String>,

    /// Dashboard API key for --push (default: $CONTEXTGUARD_API_KEY)
    #[arg(long)]
    api_key: Option<String>,
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

    if let Some(Command::Context { format }) = cli.command {
        let audit = context::audit(&sessions, &pricing);
        match format {
            ContextFormat::Text => report::print_context_audit(&audit, lang),
            ContextFormat::Json => println!("{}", report::context_audit_json(&audit)),
        }
        return;
    }

    match cli.format {
        OutputFormat::Text => {
            report::print_report(&agg, claude_md_report.as_ref(), &savings_report, lang);
            report::print_optimizations(&sessions, &pricing, claude_md_report.as_ref(), lang);
        }
        OutputFormat::Markdown => {
            report::print_markdown_report(&agg, claude_md_report.as_ref(), &sessions, &pricing, lang);
        }
    }

    if cli.push {
        let api_url = cli.api_url.or_else(|| std::env::var("CONTEXTGUARD_API_URL").ok());
        let api_key = cli.api_key.or_else(|| std::env::var("CONTEXTGUARD_API_KEY").ok());

        let (Some(api_url), Some(api_key)) = (api_url, api_key) else {
            println!();
            eprintln!("{}", i18n::err_push_missing_config(lang).red());
            std::process::exit(1);
        };

        println!();
        println!("{}", i18n::push_header(lang).bold());
        let outcomes = push::push_snapshots(&sessions, &pricing, &api_url, &api_key, savings_report.tokens_saved_estimate);
        for outcome in outcomes {
            match outcome.result {
                Ok(()) => println!("{}", i18n::push_result_line(lang, &outcome.day, true, "").green()),
                Err(e) => println!("{}", i18n::push_result_line(lang, &outcome.day, false, &e).red()),
            }
        }
    }
}
