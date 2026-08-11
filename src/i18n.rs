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

pub fn err_push_missing_config(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "--push requires both an API URL and an API key: pass --api-url/--api-key or set CONTEXTGUARD_API_URL/CONTEXTGUARD_API_KEY",
        Lang::Ru => "--push требует API URL и API-ключ: передайте --api-url/--api-key или задайте CONTEXTGUARD_API_URL/CONTEXTGUARD_API_KEY",
    }
}

pub fn push_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Pushing aggregated daily snapshots to the dashboard:",
        Lang::Ru => "Отправка агрегированных снимков в дашборд:",
    }
}

pub fn push_result_line(lang: Lang, day: &str, ok: bool, detail: &str) -> String {
    match (lang, ok) {
        (Lang::En, true) => format!("  {day}: ok"),
        (Lang::Ru, true) => format!("  {day}: успешно"),
        (Lang::En, false) => format!("  {day}: failed — {detail}"),
        (Lang::Ru, false) => format!("  {day}: ошибка — {detail}"),
    }
}

pub fn no_issues_found(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No obvious issues found — looks efficient.",
        Lang::Ru => "Явных проблем не найдено — выглядит эффективно.",
    }
}

// --- Cost-Optimization Engine ---
// Every finding renders as exactly three lines: a dollar loss (this shared
// `optimize_loss_line`), an algorithm-specific reason, and a one-line fix.

pub fn optimize_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Cost-Optimization Engine:",
        Lang::Ru => "Движок оптимизации стоимости:",
    }
}

pub fn optimize_none_found(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No token-waste patterns found — looks efficient.",
        Lang::Ru => "Паттерны потери токенов не найдены — выглядит эффективно.",
    }
}

pub fn optimize_loss_line(lang: Lang, usd: f64) -> String {
    match lang {
        Lang::En => format!("Lost ≈ ${usd:.2}"),
        Lang::Ru => format!("Потеряно ≈ ${usd:.2}"),
    }
}

pub fn optimize_action_prefix(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Fix:",
        Lang::Ru => "Действие:",
    }
}

// 1. Cache Churn Detector

pub fn optimize_cache_churn_reason(lang: Lang, session_id: &str, churn_pct: f64, turns: usize) -> String {
    match lang {
        Lang::En => format!(
            "Session {session_id}: the cache is being rewritten instead of reused — {churn_pct:.0}% of tokens on turns 2+ paid the cache-write price ({turns} turns)."
        ),
        Lang::Ru => format!(
            "Сессия {session_id}: кэш пересобирается вместо переиспользования — {churn_pct:.0}% токенов на ходах 2+ оплачены по цене записи в кэш ({turns} ходов)."
        ),
    }
}

pub fn optimize_cache_churn_action(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Keep stable content (files, CLAUDE.md) earlier in the prompt and changing content later, so the cache stops invalidating.",
        Lang::Ru => "Держите стабильный контент (файлы, CLAUDE.md) в начале запроса, а изменчивый — позже, чтобы кэш не сбрасывался.",
    }
}

// 2. Re-Read Detector

pub fn optimize_re_read_reason(lang: Lang, path: &str, count: u64, session_id: &str) -> String {
    match lang {
        Lang::En => format!("{path} was read {count} times in session {session_id} — each extra read re-sends its content into context."),
        Lang::Ru => format!("{path} прочитан {count} раз(а) в сессии {session_id} — каждое повторное чтение заново отправляет его содержимое в контекст."),
    }
}

pub fn optimize_re_read_action(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Keep the file's contents in context (or in CLAUDE.md) instead of re-reading it, or Grep for just the lines you need.",
        Lang::Ru => "Держите содержимое файла в контексте (или в CLAUDE.md) вместо повторного чтения, либо используйте Grep для нужных строк.",
    }
}

// 3. CLAUDE.md Amortizer

pub fn optimize_claude_md_reason(lang: Lang, lines: usize, tokens: usize, monthly_cost: f64) -> String {
    match lang {
        Lang::En => format!(
            "CLAUDE.md is {lines} lines (~{tokens} tokens) and gets re-read on every turn — at your current pace that's ≈${monthly_cost:.2}/month just to keep it in context."
        ),
        Lang::Ru => format!(
            "CLAUDE.md — {lines} строк (~{tokens} токенов) и перечитывается на каждом ходу — при вашем текущем темпе это ≈${monthly_cost:.2}/мес только на то, чтобы держать его в контексте."
        ),
    }
}

pub fn optimize_claude_md_action(lang: Lang, target_lines: usize) -> String {
    match lang {
        Lang::En => format!("Trim CLAUDE.md to ~{target_lines} lines — remove generic advice, keep only project-specific rules."),
        Lang::Ru => format!("Сократите CLAUDE.md примерно до {target_lines} строк — уберите общие советы, оставьте только специфичные для проекта правила."),
    }
}

// 4. Burn-Rate Watch

pub fn optimize_burn_rate_reason(lang: Lang, session_id: &str, usd_per_hour: f64, p95: f64) -> String {
    match lang {
        Lang::En => format!(
            "Session {session_id} burned ${usd_per_hour:.2}/hour — above your own p95 baseline of ${p95:.2}/hour across analyzed sessions."
        ),
        Lang::Ru => format!(
            "Сессия {session_id} жгла ${usd_per_hour:.2}/час — выше вашего собственного порога p95 в ${p95:.2}/час по проанализированным сессиям."
        ),
    }
}

pub fn optimize_burn_rate_action(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Watch this session live next time and pause or /compact as soon as $/hour spikes above your usual pace.",
        Lang::Ru => "В следующий раз следите за этой сессией вживую и делайте паузу или /compact, как только $/час резко превысит обычный темп.",
    }
}

// 5. Context Growth Advisor

pub fn optimize_context_growth_reason(lang: Lang, session_id: &str, growth_ratio: f64, turns: usize) -> String {
    match lang {
        Lang::En => format!(
            "Session {session_id}: context grew {growth_ratio:.1}x from the first turns to the last with no /compact in between ({turns} turns)."
        ),
        Lang::Ru => format!(
            "Сессия {session_id}: контекст вырос в {growth_ratio:.1} раза от первых ходов к последним без единого /compact ({turns} ходов)."
        ),
    }
}

pub fn optimize_context_growth_action(lang: Lang, optimal_turn: usize) -> String {
    match lang {
        Lang::En => format!("Run /compact starting around turn {optimal_turn} — that's roughly where the growth stopped paying for itself."),
        Lang::Ru => format!("Вызовите /compact начиная примерно с хода {optimal_turn} — именно там рост контекста перестал окупаться."),
    }
}

// 6. Model-Mismatch

pub fn optimize_model_mismatch_reason(lang: Lang, session_id: &str, flagged_turns: usize) -> String {
    match lang {
        Lang::En => format!(
            "Session {session_id} used Opus for {flagged_turns} simple edit turn(s) (short reply, a single Edit/Write call) — Opus costs several times more than Sonnet for the same tokens."
        ),
        Lang::Ru => format!(
            "В сессии {session_id} Opus использовался для {flagged_turns} простых правок (короткий ответ, один вызов Edit/Write) — Opus стоит в несколько раз дороже Sonnet за те же токены."
        ),
    }
}

pub fn optimize_model_mismatch_action(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Switch to Sonnet for straightforward edits and reserve Opus for genuinely hard reasoning.",
        Lang::Ru => "Переключитесь на Sonnet для несложных правок, а Opus оставьте для действительно сложных задач.",
    }
}

// --- Markdown report (--format markdown) ---
// Compact, Slack/GitHub-PR-friendly: a summary line plus the top 3 findings
// across all six detectors combined (not per-detector like the text report).

pub fn markdown_report_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "📊 **ContextGuard Cost Audit Report**",
        Lang::Ru => "📊 **ContextGuard Cost Audit Report**",
    }
}

pub fn markdown_summary_line(lang: Lang, sessions: usize, cost_usd: f64) -> String {
    match lang {
        Lang::En => format!("- **Sessions:** {sessions} | **Cost:** ${cost_usd:.2}"),
        Lang::Ru => format!("- **Всего сессий:** {sessions} | **Затраты:** ${cost_usd:.2}"),
    }
}

pub fn markdown_cache_hit_line(lang: Lang, pct: f64, healthy: bool) -> String {
    match (lang, healthy) {
        (Lang::En, true) => format!("- **Cache Hit Rate:** {pct:.0}% ✅"),
        (Lang::Ru, true) => format!("- **Cache Hit Rate:** {pct:.0}% ✅"),
        (Lang::En, false) => format!("- **Cache Hit Rate:** {pct:.0}% ⚠️ (Target: ≥85%)"),
        (Lang::Ru, false) => format!("- **Cache Hit Rate:** {pct:.0}% ⚠️ (Цель: ≥85%)"),
    }
}

pub fn markdown_top_issues_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "💡 **Top issues & potential savings:**",
        Lang::Ru => "💡 **Топ-3 проблемы и экономия:**",
    }
}

pub fn markdown_no_issues(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "✅ No significant token-waste patterns found.",
        Lang::Ru => "✅ Существенных потерь токенов не найдено.",
    }
}

pub fn markdown_action_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "*Fix:*",
        Lang::Ru => "*Исправление:*",
    }
}

pub fn markdown_title_cache_churn(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "🔄 **Cache Churn in a Long Session**",
        Lang::Ru => "🔄 **Cache Churn в длинной сессии**",
    }
}

pub fn markdown_loss_cache_churn(lang: Lang, loss_usd: f64, session_short: &str) -> String {
    match lang {
        Lang::En => format!("*Loss:* ${loss_usd:.2} in session `#{session_short}`"),
        Lang::Ru => format!("*Потеря:* ${loss_usd:.2} в сессии `#{session_short}`"),
    }
}

pub fn markdown_title_re_read(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "🔁 **Repeated File Reads**",
        Lang::Ru => "🔁 **Многократное чтение файла**",
    }
}

pub fn markdown_loss_re_read(lang: Lang, loss_usd: f64, path: &str, count: u64) -> String {
    match lang {
        Lang::En => format!("*Loss:* ${loss_usd:.2} (`{path}`, read {count}x)"),
        Lang::Ru => format!("*Потеря:* ${loss_usd:.2} (`{path}`, {count} раз(а))"),
    }
}

pub fn markdown_title_claude_md(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "⚠️ **CLAUDE.md is Bloated**",
        Lang::Ru => "⚠️ **CLAUDE.md перегружен**",
    }
}

pub fn markdown_loss_claude_md(lang: Lang, loss_usd: f64, lines: usize) -> String {
    match lang {
        Lang::En => format!("*Loss:* ~${loss_usd:.2}/mo ({lines} lines)"),
        Lang::Ru => format!("*Потеря:* ~${loss_usd:.2}/мес ({lines} строк)"),
    }
}

pub fn markdown_title_burn_rate(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "🔥 **High Burn Rate**",
        Lang::Ru => "🔥 **Высокий Burn Rate**",
    }
}

pub fn markdown_loss_burn_rate(lang: Lang, loss_usd: f64, session_short: &str) -> String {
    match lang {
        Lang::En => format!("*Loss:* ${loss_usd:.2} above your p95 baseline in session `#{session_short}`"),
        Lang::Ru => format!("*Потеря:* ${loss_usd:.2} сверх вашего порога p95 в сессии `#{session_short}`"),
    }
}

pub fn markdown_title_context_growth(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "📈 **Unbounded Context Growth**",
        Lang::Ru => "📈 **Неограниченный рост контекста**",
    }
}

pub fn markdown_loss_context_growth(lang: Lang, loss_usd: f64, session_short: &str) -> String {
    match lang {
        Lang::En => format!("*Loss:* ${loss_usd:.2} in session `#{session_short}`"),
        Lang::Ru => format!("*Потеря:* ${loss_usd:.2} в сессии `#{session_short}`"),
    }
}

pub fn markdown_title_model_mismatch(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "🤖 **Opus Used for Simple Edits**",
        Lang::Ru => "🤖 **Opus для простых правок**",
    }
}

pub fn markdown_loss_model_mismatch(lang: Lang, loss_usd: f64, session_short: &str, turns: usize) -> String {
    match lang {
        Lang::En => format!("*Loss:* ${loss_usd:.2} across {turns} turn(s) in session `#{session_short}`"),
        Lang::Ru => format!("*Потеря:* ${loss_usd:.2} за {turns} ход(ов) в сессии `#{session_short}`"),
    }
}

// --- context audit -----------------------------------------------------

pub fn context_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "What is in your context window",
        Lang::Ru => "Что занимает ваше контекстное окно",
    }
}

pub fn context_subtitle(lang: Lang, sessions: usize) -> String {
    match lang {
        Lang::En => format!("Averaged across {sessions} session(s) on this machine"),
        Lang::Ru => format!("Усреднено по {sessions} сессиям на этой машине"),
    }
}

pub fn context_nothing(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No sessions with usable content found.",
        Lang::Ru => "Сессий с пригодным содержимым не найдено.",
    }
}

pub fn context_estimated_marker(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "estimated",
        Lang::Ru => "оценка",
    }
}

pub fn context_carried_note(lang: Lang, tokens: &str, cost: &str) -> String {
    match lang {
        Lang::En => format!(
            "Re-sent across every request in these sessions: {tokens} tokens, about {cost}.\n\
             That is what the window costs you, as opposed to what it holds."
        ),
        Lang::Ru => format!(
            "Переслано заново во всех запросах этих сессий: {tokens} токенов, примерно {cost}.\n\
             Это то, во что окно обходится, а не то, что в нём лежит."
        ),
    }
}

pub fn context_prefix_note(lang: Lang, tokens: &str) -> String {
    match lang {
        Lang::En => format!(
            "The system prompt and tool schemas are never written to the transcript.\n\
             {tokens} tokens is what remains after subtracting everything visible from\n\
             the first request's own token count — an estimate, not a measurement."
        ),
        Lang::Ru => format!(
            "Системный промпт и схемы инструментов в транскрипт не пишутся.\n\
             {tokens} токенов — это остаток после вычитания всего видимого из счётчика\n\
             токенов первого запроса. Это оценка, а не измерение."
        ),
    }
}

pub fn context_mcp_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "MCP servers that answered a call:",
        Lang::Ru => "MCP-серверы, которые отвечали на вызовы:",
    }
}

pub fn context_mcp_caveat(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Every configured server's schemas sit in the fixed prefix above whether \
or not\n  they are used. How many tokens each one costs is not in the transcript — only\n  \
which ones went unused.",
        Lang::Ru => "Схемы каждого настроенного сервера лежат в фиксированном префиксе выше \
независимо\n  от использования. Сколько токенов стоит каждый — в транскрипте нет; есть только\n  \
то, какие не пригодились.",
    }
}

pub fn context_rereads_header(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Files read more than once in a single session:",
        Lang::Ru => "Файлы, прочитанные больше одного раза за сессию:",
    }
}

pub fn context_reread_line(lang: Lang, count: usize, path: &str) -> String {
    match lang {
        Lang::En => format!("{count}x  {path}"),
        Lang::Ru => format!("{count}x  {path}"),
    }
}

pub fn context_mcp_unused(lang: Lang, count: usize, names: &str) -> String {
    match lang {
        Lang::En => format!("{count} configured server(s) were never called: {names}"),
        Lang::Ru => format!("{count} настроенных сервер(ов) не вызывались ни разу: {names}"),
    }
}

pub fn err_write_file(lang: Lang, path: &str, e: &str) -> String {
    match lang {
        Lang::En => format!("could not write {path}: {e}"),
        Lang::Ru => format!("не удалось записать {path}: {e}"),
    }
}

// --- lint ----------------------------------------------------------------

pub fn lint_title(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("CLAUDE.md lint — {path}"),
        Lang::Ru => format!("Проверка CLAUDE.md — {path}"),
    }
}

pub fn lint_summary(lang: Lang, lines: usize, tokens: u64) -> String {
    match lang {
        Lang::En => format!("{lines} lines, ~{tokens} tokens"),
        Lang::Ru => format!("{lines} строк, ~{tokens} токенов"),
    }
}

pub fn lint_clean(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No issues found.",
        Lang::Ru => "Замечаний не найдено.",
    }
}

pub fn lint_reason_boilerplate(lang: Lang, phrase: &str) -> String {
    match lang {
        Lang::En => format!("restates default behavior (\"{phrase}\")"),
        Lang::Ru => format!("повторяет поведение модели по умолчанию («{phrase}»)"),
    }
}

pub fn lint_reason_duplicate(lang: Lang, origin_line: usize) -> String {
    match lang {
        Lang::En => format!("identical to line {origin_line}"),
        Lang::Ru => format!("совпадает со строкой {origin_line}"),
    }
}

pub fn lint_reason_stale_path(lang: Lang, token: &str) -> String {
    match lang {
        Lang::En => format!("`{token}` — no analyzed session touched this path"),
        Lang::Ru => format!("«{token}» — ни одна проанализированная сессия не обращалась к этому пути"),
    }
}

pub fn lint_reason_unused_server(lang: Lang, server: &str) -> String {
    match lang {
        Lang::En => format!("`{server}` was never called in the analyzed sessions"),
        Lang::Ru => format!("«{server}» ни разу не вызывался в проанализированных сессиях"),
    }
}

pub fn lint_kind_label(lang: Lang, kind: crate::lint::FindingKind) -> &'static str {
    use crate::lint::FindingKind::*;
    match (lang, kind) {
        (Lang::En, Boilerplate) => "boilerplate",
        (Lang::Ru, Boilerplate) => "шаблонная фраза",
        (Lang::En, Duplicate) => "duplicate",
        (Lang::Ru, Duplicate) => "дубликат",
        (Lang::En, StalePath) => "stale path",
        (Lang::Ru, StalePath) => "неиспользуемый путь",
        (Lang::En, UnusedMcpServer) => "unused MCP server",
        (Lang::Ru, UnusedMcpServer) => "неиспользуемый MCP-сервер",
    }
}

pub fn lint_fixable_note(lang: Lang, count: usize, tokens: u64) -> String {
    match lang {
        Lang::En => format!("{count} of these can be removed automatically with --fix (~{tokens} tokens)"),
        Lang::Ru => format!("{count} из них можно удалить автоматически флагом --fix (~{tokens} токенов)"),
    }
}

pub fn lint_cost_per_1k(lang: Lang, usd: f64) -> String {
    match lang {
        Lang::En => format!("${usd:.4} per 1,000 requests at Anthropic's published Sonnet cache-read rate"),
        Lang::Ru => format!("${usd:.4} за 1000 запросов по опубликованному тарифу Anthropic (Sonnet, чтение из кэша)"),
    }
}

pub fn lint_monthly_cost(lang: Lang, usd: f64) -> String {
    match lang {
        Lang::En => format!("≈${usd:.2}/mo at the volume observed in the sessions analyzed"),
        Lang::Ru => format!("≈${usd:.2}/мес при объёме, замеренном в проанализированных сессиях"),
    }
}

pub fn lint_monthly_savings(lang: Lang, usd: f64) -> String {
    match lang {
        Lang::En => format!("--fix would save about ${usd:.2}/mo at that volume"),
        Lang::Ru => format!("--fix сэкономил бы примерно ${usd:.2}/мес при этом объёме"),
    }
}

pub fn lint_no_local_volume(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No local session history to measure a monthly volume from — run this on a \
machine with Claude Code sessions to see a $/month figure.",
        Lang::Ru => "Нет локальной истории сессий, чтобы измерить месячный объём — запустите \
на машине с сессиями Claude Code, чтобы увидеть оценку $/мес.",
    }
}

pub fn lint_fix_nothing(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Nothing auto-fixable found — no changes made.",
        Lang::Ru => "Нечего исправлять автоматически — файл не изменён.",
    }
}

pub fn lint_fix_preview_header(lang: Lang, count: usize) -> String {
    match lang {
        Lang::En => format!("--fix will remove {count} line(s):"),
        Lang::Ru => format!("--fix удалит {count} строк(и):"),
    }
}

pub fn lint_fix_done(lang: Lang, removed: usize, remaining: usize) -> String {
    match lang {
        Lang::En => format!("Done — {removed} line(s) removed, {remaining} remain."),
        Lang::Ru => format!("Готово — удалено строк: {removed}, осталось: {remaining}."),
    }
}

pub fn lint_compare_title(lang: Lang, baseline: &str, current: &str) -> String {
    match lang {
        Lang::En => format!("CLAUDE.md changed: {baseline} → {current}"),
        Lang::Ru => format!("CLAUDE.md изменён: {baseline} → {current}"),
    }
}

pub fn lint_compare_delta(lang: Lang, delta: i64, from: u64, to: u64) -> String {
    let sign = if delta >= 0 { "+" } else { "" };
    match lang {
        Lang::En => format!("{sign}{delta} tokens (from {from} to {to}) on every request"),
        Lang::Ru => format!("{sign}{delta} токенов (было {from}, стало {to}) в каждом запросе"),
    }
}

pub fn lint_compare_no_change(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "CLAUDE.md's token count did not change.",
        Lang::Ru => "Количество токенов в CLAUDE.md не изменилось.",
    }
}

pub fn lint_compare_price_added(lang: Lang, usd_per_1k: f64) -> String {
    match lang {
        Lang::En => format!("That's about ${usd_per_1k:.2} per 1,000 requests at Anthropic's published Sonnet cache-read rate."),
        Lang::Ru => format!("Это примерно ${usd_per_1k:.2} за 1000 запросов по опубликованному тарифу Anthropic (Sonnet, чтение из кэша)."),
    }
}

pub fn lint_compare_price_saved(lang: Lang, usd_per_1k: f64) -> String {
    match lang {
        Lang::En => format!("That saves about ${usd_per_1k:.2} per 1,000 requests at Anthropic's published Sonnet cache-read rate."),
        Lang::Ru => format!("Это экономит примерно ${usd_per_1k:.2} за 1000 запросов по опубликованному тарифу Anthropic (Sonnet, чтение из кэша)."),
    }
}

pub fn lint_compare_monthly(lang: Lang, usd: f64) -> String {
    match lang {
        Lang::En => format!("At the volume observed in the sessions analyzed, that's about ${usd:.2}/mo."),
        Lang::Ru => format!("При объёме, замеренном в проанализированных сессиях, это примерно ${usd:.2}/мес."),
    }
}

pub fn lint_compare_footer(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Run `contextguard lint --compare-to <base> CLAUDE.md` locally to price this at your team's actual request volume.",
        Lang::Ru => "Запустите `contextguard lint --compare-to <base> CLAUDE.md` локально, чтобы оценить это при реальном объёме запросов команды.",
    }
}

// --- savings report --------------------------------------------------------

const EN_MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];
// Genitive case ("of August"), which is what "<Month> <Year>" grammatically
// needs in Russian — the nominative "Август" would read as ungrammatical
// next to a year the way English doesn't mark this distinction at all.
const RU_MONTHS_GENITIVE: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября",
    "декабря",
];

/// "2026-08" -> "August 2026" / "августа 2026". Falls back to the raw
/// "YYYY-MM" key itself for anything that doesn't parse — a display quirk,
/// not a reason to drop data that was otherwise groupable.
pub fn savings_month_label(lang: Lang, month_key: &str) -> String {
    let parsed = month_key.split_once('-').and_then(|(y, m)| Some((y, m.parse::<usize>().ok()?)));
    match parsed {
        Some((year, m)) if (1..=12).contains(&m) => match lang {
            Lang::En => format!("{} {year}", EN_MONTHS[m - 1]),
            Lang::Ru => format!("{} {year}", RU_MONTHS_GENITIVE[m - 1]),
        },
        _ => month_key.to_string(),
    }
}

pub fn savings_title(lang: Lang, month_label: &str) -> String {
    match lang {
        Lang::En => format!("Savings report — {month_label}"),
        Lang::Ru => format!("Отчёт об экономии — {month_label}"),
    }
}

pub fn savings_headline(lang: Lang, tokens: u64, usd: f64) -> String {
    match lang {
        Lang::En => format!("Saved by the plugin: {tokens} tokens ≈ ${usd:.2}"),
        Lang::Ru => format!("Сэкономлено плагином: {tokens} токенов ≈ ${usd:.2}"),
    }
}

pub fn savings_amortized_note(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "(amortized: priced by how many turns of the session were still ahead of each intervention)",
        Lang::Ru => "(с амортизацией: оценено по числу ходов сессии, оставшихся после каждого вмешательства)",
    }
}

pub fn savings_from_line(lang: Lang, interventions: u64, sessions: usize) -> String {
    match lang {
        Lang::En => format!("From {interventions} intervention(s) across {sessions} session(s) this month"),
        Lang::Ru => format!("Из {interventions} вмешательств(а) в {sessions} сессии(ях) за месяц"),
    }
}

pub fn savings_partial_amortization_note(lang: Lang, amortized: u64, total: u64) -> String {
    match lang {
        Lang::En => format!(
            "{amortized} of {total} could be matched to a session and amortized; \
the rest are counted once, which understates their real value."
        ),
        Lang::Ru => format!(
            "{amortized} из {total} удалось сопоставить с сессией и амортизировать; \
остальные учтены один раз, то есть их реальная ценность занижена."
        ),
    }
}

pub fn savings_no_amortization_note(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "None of these could be matched to a locally-parsed session (older plugin \
version, or the session has since rotated out of --days), so each is counted once — a floor, \
not the real amortized value.",
        Lang::Ru => "Ни одно не удалось сопоставить с локально разобранной сессией (старая версия \
плагина, либо сессия уже выпала из --days), поэтому каждое учтено один раз — это нижняя оценка, \
а не реальная амортизированная ценность.",
    }
}

pub fn savings_top_command(lang: Lang, label: &str, tokens: u64) -> String {
    match lang {
        Lang::En => format!("Top source: {label} output truncated — {tokens} tokens"),
        Lang::Ru => format!("Главный источник: обрезка вывода «{label}» — {tokens} токенов"),
    }
}

pub fn savings_grep_cap_line(lang: Lang, count: u64) -> String {
    match lang {
        Lang::En => format!(
            "Also capped {count} unbounded Grep search(es) this month — no token estimate: \
nothing to compare against without running the search twice."
        ),
        Lang::Ru => format!(
            "Также ограничено {count} неограниченных поисков Grep за месяц — без оценки токенов: \
не с чем сравнивать без повторного запуска поиска."
        ),
    }
}

pub fn savings_nothing_this_month(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No bash-output truncations this month.",
        Lang::Ru => "В этом месяце обрезок вывода Bash не было.",
    }
}

pub fn savings_empty(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No plugin activity found at all. Install the plugin \
(github.com/ChevvyOkK/contextguard-plugin) and this report fills in as it runs.",
        Lang::Ru => "Активности плагина не найдено вообще. Установите плагин \
(github.com/ChevvyOkK/contextguard-plugin) — отчёт начнёт заполняться по мере его работы.",
    }
}

// --- budget ----------------------------------------------------------------

pub fn budget_status_line(lang: Lang, period_key: &str, spend_usd: f64, max_usd: f64) -> String {
    match lang {
        Lang::En => format!("{period_key}: ${spend_usd:.2} spent of ${max_usd:.2} budget"),
        Lang::Ru => format!("{period_key}: потрачено ${spend_usd:.2} из ${max_usd:.2} бюджета"),
    }
}

pub fn budget_crossed_note(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Threshold crossed.",
        Lang::Ru => "Порог превышен.",
    }
}

pub fn budget_webhook_sent(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Webhook notified.",
        Lang::Ru => "Уведомление отправлено на webhook.",
    }
}

pub fn budget_webhook_failed(lang: Lang, e: &str) -> String {
    match lang {
        Lang::En => format!("Could not notify the webhook: {e}"),
        Lang::Ru => format!("Не удалось отправить уведомление на webhook: {e}"),
    }
}

// --- git-cost ----------------------------------------------------------------

pub fn err_git_cost(lang: Lang, e: &str) -> String {
    match lang {
        Lang::En => format!("Could not attribute cost by git history: {e}"),
        Lang::Ru => format!("Не удалось посчитать стоимость по git-истории: {e}"),
    }
}

pub fn git_cost_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Cost by git branch / PR",
        Lang::Ru => "Стоимость по веткам / PR",
    }
}

pub fn git_cost_subtitle(lang: Lang, base: &str, lookback_hours: f64) -> String {
    // Avoid "0h" for a genuine sub-hour lookback (e.g. 0.5h) rounding away
    // to something that reads as "no lookback at all".
    let lookback = if lookback_hours.fract().abs() < 0.05 {
        format!("{lookback_hours:.0}h")
    } else {
        format!("{lookback_hours:.1}h")
    };
    match lang {
        Lang::En => format!("base branch: {base}, ~{lookback} lookback before each first commit"),
        Lang::Ru => format!("базовая ветка: {base}, запас ~{lookback} до первого коммита"),
    }
}

pub fn git_cost_empty(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No local branches or merged PRs with matching local session data found.",
        Lang::Ru => "Не найдено веток или влитых PR с данными локальных сессий.",
    }
}

pub fn git_cost_squash_marker(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "squash — no sub-commit history",
        Lang::Ru => "squash — история под-коммитов недоступна",
    }
}

pub fn git_cost_commits(lang: Lang, n: usize) -> String {
    match lang {
        Lang::En => format!("{n} commit{}", if n == 1 { "" } else { "s" }),
        Lang::Ru => format!("{n} коммит{}", ru_commit_suffix(n)),
    }
}

/// The trailing metadata segment of a git-cost report line: commit count,
/// how many session turns backed the cost figure (a small number is a
/// weaker estimate — worth showing, not hiding), and the date work on this
/// item is reckoned to have started from.
pub fn git_cost_item_meta(lang: Lang, commit_count: usize, turns_counted: usize, since_date: &str) -> String {
    let commits = git_cost_commits(lang, commit_count);
    match lang {
        Lang::En => format!("{commits}, {turns_counted} turn(s) matched, since {since_date}"),
        Lang::Ru => format!("{commits}, совпало реплик: {turns_counted}, с {since_date}"),
    }
}

fn ru_commit_suffix(n: usize) -> &'static str {
    let n100 = n % 100;
    let n10 = n % 10;
    if (11..=14).contains(&n100) {
        "ов"
    } else if n10 == 1 {
        ""
    } else if (2..=4).contains(&n10) {
        "а"
    } else {
        "ов"
    }
}

pub fn git_cost_footer(lang: Lang, turns_found: usize) -> String {
    match lang {
        Lang::En => format!(
            "{turns_found} local session turn(s) matched this repo. Cost is a time-window estimate, not an exact trace — \
             working on two branches inside the same lookback window can attribute a turn to both."
        ),
        Lang::Ru => format!(
            "{turns_found} совпадающих реплик локальных сессий для этого репозитория. Стоимость — оценка по временным окнам, \
             не точная трассировка: если работа шла над двумя ветками в одном окне запаса, реплика может попасть в обе."
        ),
    }
}

pub fn git_cost_no_local_sessions(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "No local Claude Code sessions matched this repository's directory at all — cost figures below are $0 for lack of data, not because nothing happened.",
        Lang::Ru => "Ни одна локальная сессия Claude Code не найдена для директории этого репозитория — цифры ниже равны $0 из-за отсутствия данных, а не потому что ничего не происходило.",
    }
}

