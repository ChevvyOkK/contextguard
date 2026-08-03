/// Defaults to English since that's the actual Claude Code user base this
/// tool targets; Russian is opt-in via `--lang ru` or `CONTEXTGUARD_LANG=ru`
/// rather than guessed from OS locale, which is unreliable on Windows and
/// would silently surprise an English-speaking user who happens to have a
/// non-English system locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    pub fn detect(cli_override: Option<&str>) -> Lang {
        if let Some(code) = cli_override {
            return Lang::from_code(code);
        }
        if let Ok(code) = std::env::var("CONTEXTGUARD_LANG") {
            return Lang::from_code(&code);
        }
        Lang::En
    }

    fn from_code(code: &str) -> Lang {
        if code.to_ascii_lowercase().starts_with("ru") {
            Lang::Ru
        } else {
            Lang::En
        }
    }
}

pub fn err_home_dir_not_found(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "could not determine the home directory",
        Lang::Ru => "не удалось определить домашнюю директорию",
    }
}

pub fn err_finding_session_files(lang: Lang, e: &str) -> String {
    match lang {
        Lang::En => format!("Error finding session files: {e}"),
        Lang::Ru => format!("Ошибка поиска файлов сессий: {e}"),
    }
}

pub fn no_session_files_found(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No Claude Code session files found (~/.claude/projects/). Nothing to analyze.",
        Lang::Ru => "Файлы сессий Claude Code не найдены (~/.claude/projects/). Ничего анализировать.",
    }
}

/// `path` is expected to already be `{:?}`-formatted by the caller (paths
/// are Debug, not Display) — this only interpolates it, not re-formats it.
pub fn err_open_file(lang: Lang, path: &str, e: &str) -> String {
    match lang {
        Lang::En => format!("could not open {path}: {e}"),
        Lang::Ru => format!("не удалось открыть {path}: {e}"),
    }
}

pub fn err_read_file(lang: Lang, path: &str, e: &str) -> String {
    match lang {
        Lang::En => format!("could not read {path}: {e}"),
        Lang::Ru => format!("не удалось прочитать {path}: {e}"),
    }
}

pub fn skip_file(lang: Lang, file: &str, e: &str) -> String {
    match lang {
        Lang::En => format!("Skipping {file:?}: {e}"),
        Lang::Ru => format!("Пропуск {file:?}: {e}"),
    }
}

pub fn report_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "ContextGuard — Claude Code token usage audit",
        Lang::Ru => "ContextGuard — аудит использования Claude Code",
    }
}

pub fn sessions_analyzed(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Sessions analyzed:",
        Lang::Ru => "Сессий проанализировано:",
    }
}

pub fn tokens_line(lang: Lang, input: &str, cache_write: &str, cache_read: &str, output: &str) -> String {
    match lang {
        Lang::En => format!(
            "Tokens — input: {input} | cache-write: {cache_write} | cache-read: {cache_read} | output: {output}"
        ),
        Lang::Ru => format!(
            "Токены — входящие: {input} | кэш-запись: {cache_write} | кэш-чтение: {cache_read} | исходящие: {output}"
        ),
    }
}

pub fn cost_estimate_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Estimated cost:",
        Lang::Ru => "Оценка стоимости:",
    }
}

pub fn cache_efficiency_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Cache efficiency:",
        Lang::Ru => "Эффективность кэша:",
    }
}

pub fn cache_efficiency_warning(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "less than half of input tokens are reused from cache — that's expensive",
        Lang::Ru => "меньше половины входящих токенов переиспользуются из кэша, это дорого",
    }
}

pub fn plugin_savings_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "The ContextGuard plugin is already saving tokens:",
        Lang::Ru => "Плагин ContextGuard уже экономит токены:",
    }
}

pub fn plugin_savings_line(lang: Lang, interventions: u64, tokens: &str) -> String {
    match lang {
        Lang::En => format!("  {interventions} interventions, ~{tokens} tokens saved (estimate)"),
        Lang::Ru => format!("  {interventions} вмешательств, ~{tokens} токенов сэкономлено (оценка)"),
    }
}

pub fn top_sessions_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Most expensive sessions:",
        Lang::Ru => "Самые дорогие сессии:",
    }
}

pub fn tools_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Tools by call frequency:",
        Lang::Ru => "Инструменты по частоте вызова:",
    }
}

pub fn claude_md_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "CLAUDE.md:",
        Lang::Ru => "CLAUDE.md:",
    }
}

pub fn claude_md_path_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "  Path:",
        Lang::Ru => "  Путь:",
    }
}

pub fn claude_md_length_line(lang: Lang, lines: usize, tokens: usize) -> String {
    match lang {
        Lang::En => format!("  Length: {lines} lines (~{tokens} tokens)"),
        Lang::Ru => format!("  Длина: {lines} строк (~{tokens} токенов)"),
    }
}

pub fn claude_md_over_recommended(lang: Lang, max: usize) -> String {
    match lang {
        Lang::En => format!("— longer than the recommended {max}"),
        Lang::Ru => format!("— больше рекомендуемых {max}"),
    }
}

pub fn claude_md_generic_lines_intro(lang: Lang, count: usize) -> String {
    match lang {
        Lang::En => format!("  {count} lines look like generic advice the model already knows:"),
        Lang::Ru => format!("  {count} строк выглядят как общие фразы, которые модель и так знает:"),
    }
}

pub fn claude_md_line_label(lang: Lang, line_no: usize) -> String {
    match lang {
        Lang::En => format!("line {line_no}"),
        Lang::Ru => format!("строка {line_no}"),
    }
}

pub fn suggestions_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Suggestions:",
        Lang::Ru => "Что можно улучшить:",
    }
}

pub fn suggestion_cache_efficiency(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Low cache efficiency — keep stable content (CLAUDE.md, system instructions) at the start of the prompt and changing content at the end, so the cache gets reused more often",
        Lang::Ru => "Низкая эффективность кэша — держите стабильный контент (CLAUDE.md, системные инструкции) в начале запроса, а изменчивый — в конце, чтобы кэш переиспользовался чаще",
    }
}

pub fn suggestion_claude_md_length(lang: Lang, max: usize) -> String {
    match lang {
        Lang::En => format!("CLAUDE.md is longer than {max} lines — trim generic advice, keep only what's project-specific"),
        Lang::Ru => format!("CLAUDE.md длиннее {max} строк — уберите общие советы, оставьте только специфичное для проекта"),
    }
}

pub fn suggestion_claude_md_generic(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Remove the generic phrases from CLAUDE.md — the model already knows this, it's just wasted tokens on every session",
        Lang::Ru => "Уберите общие фразы из CLAUDE.md — модель и так это умеет, это просто трата токенов на каждой сессии",
    }
}

pub fn no_issues_found(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No obvious issues found — looks efficient.",
        Lang::Ru => "Явных проблем не найдено — выглядит эффективно.",
    }
}
