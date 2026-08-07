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
