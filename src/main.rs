mod claude_md;
mod discovery;
mod pricing;
mod report;
mod session;

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;

/// ContextGuard — локальный аудит трат токенов Claude Code. Ничего не
/// отправляет никуда: читает только локальные файлы сессий на этой машине.
#[derive(Parser, Debug)]
#[command(name = "contextguard", version, about)]
struct Cli {
    /// Учитывать только сессии за последние N дней (по умолчанию — все)
    #[arg(long)]
    days: Option<u64>,

    /// Путь к CLAUDE.md для анализа (по умолчанию ищется в текущей директории)
    #[arg(long)]
    claude_md: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let files = match discovery::find_session_files(cli.days) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("{}", format!("Ошибка поиска файлов сессий: {e}").red());
            std::process::exit(1);
        }
    };

    if files.is_empty() {
        println!(
            "{}",
            "Файлы сессий Claude Code не найдены (~/.claude/projects/). Ничего анализировать.".yellow()
        );
        return;
    }

    let mut sessions = Vec::with_capacity(files.len());
    for file in &files {
        match session::parse_session_file(file) {
            Ok(stats) => sessions.push(stats),
            Err(e) => eprintln!("{}", format!("Пропуск {file:?}: {e}").dimmed()),
        }
    }

    let pricing = pricing::PricingTable::defaults();
    let agg = report::aggregate(&sessions, &pricing);

    let claude_md_path = cli.claude_md.or_else(|| {
        let candidate = std::env::current_dir().ok()?.join("CLAUDE.md");
        candidate.exists().then_some(candidate)
    });
    let claude_md_report = claude_md_path.as_deref().and_then(|p| claude_md::analyze(p).ok());

    report::print_report(&agg, claude_md_report.as_ref());
}
