//! Priority inferred from section headings such as `#### P0` or `## High`.
//!
//! Many hand-written task files rank work by grouping it under priority
//! headings instead of tagging each line with `!n`. This module reads that
//! structure as a *derived* view: nothing here writes the file, and an explicit
//! `!n` token on a task always wins over the heading it sits under.
//!
//! The algorithm has three independent steps:
//!
//! 1. **Label recognition** ([`parse_priority_label`]): decide whether one
//!    heading text carries a priority label, and on which scale.
//! 2. **Scale normalisation**: map each label onto the native `!1`..`!4` tiers.
//!    `P`-numbers are zero- or one-based per document (see [`p_scale_origin`]);
//!    words map onto fixed tiers.
//! 3. **Section scoping** ([`heading_priorities`]): a task inherits the label of
//!    the nearest enclosing heading that has one. A heading's section ends at the
//!    next heading of the same or a higher level.
//!
//! See `docs/heading-priority-spec.md` for the rules and their rationale.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::{Document, TokenKind};

/// The family a heading label belongs to. Each family normalises differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityScale {
    /// `P0`, `P1`, `Priority 2`, `优先级 1` — lower number is more urgent;
    /// whether the top is `P0` or `P1` is decided per document.
    PNumber,
    /// `Critical` / `High` / `Medium` / `Low` and their Chinese forms,
    /// on fixed tiers 0..=3.
    Word,
    /// `!1`..`!4` — this project's own token used as a heading.
    Native,
}

/// One recognised label, before per-document normalisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityLabel {
    pub scale: PriorityScale,
    /// `P`-number for [`PriorityScale::PNumber`], tier 0..=3 for
    /// [`PriorityScale::Word`], level 1..=4 for [`PriorityScale::Native`].
    pub rank: u8,
    /// Display form: `P0`, `!2`, or the matched words (e.g. `高优先级`).
    pub raw: String,
}

/// Priority a task inherits from its enclosing heading.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HeadingPriority {
    /// Normalised native level, 1 (highest) ..= 4.
    pub level: u8,
    /// The label as recognised, e.g. `P0`.
    pub label: String,
    /// Full heading text, e.g. `P1 · 线 B：领域事件`.
    pub heading: String,
    /// 1-based line number of the heading in the source file.
    pub heading_line: usize,
    /// True when the source scale is deeper than four tiers and this label was
    /// folded into `!4`; ordering among folded labels is lost.
    pub clamped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrioritySource {
    /// A `!n` token on the task line itself.
    Explicit,
    /// Inherited from a labelled heading.
    Heading(HeadingPriority),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EffectivePriority {
    pub level: u8,
    pub source: PrioritySource,
}

/// Heading-inferred priority for every task, indexed like `document.tasks`.
///
/// Explicit `!n` tokens are ignored here; see [`effective_priorities`].
pub fn heading_priorities(document: &Document) -> Vec<Option<HeadingPriority>> {
    let body_start = document.frontmatter.range.end;
    let body = &document.source[body_start..];
    let scan = scan_body(body);
    let origin = p_scale_origin(&scan);

    let mut stack: Vec<(u8, Option<HeadingPriority>)> = Vec::new();
    let mut next_heading = 0;
    document
        .tasks
        .iter()
        .map(|task| {
            while let Some(heading) = scan.headings.get(next_heading) {
                if body_start + heading.start >= task.line_range.start {
                    break;
                }
                while stack
                    .last()
                    .is_some_and(|(level, _)| *level >= heading.level)
                {
                    stack.pop();
                }
                let inferred = heading.label.as_ref().map(|label| {
                    let (level, clamped) = normalise(label, origin);
                    HeadingPriority {
                        level,
                        label: label.raw.clone(),
                        heading: heading.text.clone(),
                        heading_line: line_number(&document.source, body_start + heading.start),
                        clamped,
                    }
                });
                stack.push((heading.level, inferred));
                next_heading += 1;
            }
            stack.iter().rev().find_map(|(_, found)| found.clone())
        })
        .collect()
}

/// Effective priority for every task: its own `!n` token when present,
/// otherwise the heading-inferred priority.
pub fn effective_priorities(document: &Document) -> Vec<Option<EffectivePriority>> {
    document
        .tasks
        .iter()
        .zip(heading_priorities(document))
        .map(|(task, inferred)| {
            // Last token wins, matching how the desktop view reads duplicates.
            let explicit = task
                .metadata
                .iter()
                .rev()
                .find_map(|token| match token.kind {
                    TokenKind::Priority(level) => Some(level),
                    _ => None,
                });
            match (explicit, inferred) {
                (Some(level), _) => Some(EffectivePriority {
                    level,
                    source: PrioritySource::Explicit,
                }),
                (None, Some(heading)) => Some(EffectivePriority {
                    level: heading.level,
                    source: PrioritySource::Heading(heading),
                }),
                (None, None) => None,
            }
        })
        .collect()
}

/// Recognise a priority label in one heading's text.
///
/// The label must sit at the head of the heading (after optional numbering such
/// as `1.` / `三、` and decoration such as an emoji), or at its tail, set off by
/// brackets or a separator. Bare words such as `High` count only at the head or
/// inside brackets; anywhere else a label must be distinctive (`P1`, `!2`, or
/// carry a keyword like `优先级` / `priority`), because `Availability: High` is
/// a description, not a rank.
pub fn parse_priority_label(text: &str) -> Option<PriorityLabel> {
    let normalized = normalize_width(text);
    let text = strip_leading_decoration(&normalized);
    head_label(text).or_else(|| tail_label(text))
}

// ---------------------------------------------------------------------------
// Scale normalisation

/// The number of the most urgent `P` tier in this document: 0 or 1.
///
/// `P0`-first and `P1`-first conventions are both common. A document is
/// zero-based when `P0` appears anywhere in its prose or headings — a legend
/// such as `**P0：正确性与安全阻断**` counts even after every P0 task is gone,
/// which keeps `P1` from being silently promoted to the top tier. Inline and
/// fenced code are excluded.
fn p_scale_origin(scan: &BodyScan) -> u8 {
    let heading_p0 = scan.headings.iter().any(|heading| {
        heading
            .label
            .as_ref()
            .is_some_and(|label| label.scale == PriorityScale::PNumber && label.rank == 0)
    });
    u8::from(!(heading_p0 || scan.prose_mentions_p0))
}

fn normalise(label: &PriorityLabel, origin: u8) -> (u8, bool) {
    match label.scale {
        PriorityScale::PNumber => {
            let level = u16::from(label.rank.saturating_sub(origin)) + 1;
            (level.min(4) as u8, level > 4)
        }
        PriorityScale::Word => (label.rank + 1, false),
        PriorityScale::Native => (label.rank, false),
    }
}

// ---------------------------------------------------------------------------
// Document scan

struct ScannedHeading {
    level: u8,
    /// Byte offset relative to the body (after frontmatter).
    start: usize,
    text: String,
    label: Option<PriorityLabel>,
}

struct BodyScan {
    headings: Vec<ScannedHeading>,
    prose_mentions_p0: bool,
}

/// Walk the Markdown body once. The frontmatter is excluded because its closing
/// `---` would otherwise turn the last YAML line into a setext heading.
fn scan_body(body: &str) -> BodyScan {
    let mut headings = Vec::new();
    let mut current: Option<(u8, usize, String)> = None;
    let mut in_code_block = false;
    let mut prose_mentions_p0 = false;
    for (event, range) in Parser::new_ext(body, Options::ENABLE_TASKLISTS).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
            Event::End(TagEnd::CodeBlock) => in_code_block = false,
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((heading_level(level), range.start, String::new()));
            }
            Event::Text(text) if !in_code_block => {
                prose_mentions_p0 |= mentions_p0(&text);
                if let Some((_, _, value)) = &mut current {
                    value.push_str(&text);
                }
            }
            Event::Code(text) => {
                if let Some((_, _, value)) = &mut current {
                    value.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, start, text)) = current.take() {
                    let text = text.trim().to_string();
                    let label = parse_priority_label(&text);
                    headings.push(ScannedHeading {
                        level,
                        start,
                        text,
                        label,
                    });
                }
            }
            _ => {}
        }
    }
    BodyScan {
        headings,
        prose_mentions_p0,
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A standalone `P0` token: not part of a longer ASCII word or number.
fn mentions_p0(text: &str) -> bool {
    let text = normalize_width(text);
    let chars: Vec<char> = text.chars().collect();
    chars.windows(2).enumerate().any(|(index, pair)| {
        matches!(pair[0], 'P' | 'p')
            && pair[1] == '0'
            && (index == 0 || !chars[index - 1].is_ascii_alphanumeric())
            && chars
                .get(index + 2)
                .is_none_or(|next| !next.is_ascii_alphanumeric())
    })
}

fn line_number(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

// ---------------------------------------------------------------------------
// Label recognition

/// Fold full-width ASCII (`Ｐ０`, `：`, `（`) to half-width so one grammar covers
/// both. Width-only folding; case is left for the matchers.
fn normalize_width(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\u{FF01}'..='\u{FF5E}' => {
                char::from_u32(character as u32 - 0xFEE0).unwrap_or(character)
            }
            '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

const OPEN_BRACKETS: [char; 4] = ['(', '[', '【', '〔'];

fn closing_bracket(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        '【' => '】',
        _ => '〕',
    }
}

/// Drop leading emoji/symbols and section numbering (`1.`, `1.2`, `三、`, `(一)`).
fn strip_leading_decoration(mut text: &str) -> &str {
    loop {
        let before = text;
        text = text.trim_start_matches(|character: char| {
            !character.is_alphanumeric() && !OPEN_BRACKETS.contains(&character) && character != '!'
        });
        text = strip_enumeration(text);
        if text.len() == before.len() {
            return text;
        }
    }
}

fn is_cjk_numeral(character: char) -> bool {
    "一二三四五六七八九十百零".contains(character)
}

fn strip_enumeration(text: &str) -> &str {
    // `(1)` / `(一)`
    if let Some(rest) = text.strip_prefix('(') {
        if let Some(close) = rest.find(')') {
            let inner = &rest[..close];
            if !inner.is_empty()
                && inner
                    .chars()
                    .all(|character| character.is_ascii_digit() || is_cjk_numeral(character))
            {
                return rest[close + 1..].trim_start();
            }
        }
        return text;
    }
    // `1.` `1)` `1、` `1.2.3` followed by whitespace or a delimiter
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let mut end = digits;
        while text[end..].starts_with('.')
            && text[end + 1..]
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            end += 1 + text[end + 1..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
        }
        let rest = &text[end..];
        if let Some(after) = rest
            .strip_prefix(['.', ')', '、'])
            .or_else(|| (end > digits && rest.starts_with(' ')).then_some(rest))
        {
            return after.trim_start();
        }
        return text;
    }
    // `三、` `十二.`
    let numerals: usize = text
        .chars()
        .take_while(|character| is_cjk_numeral(*character))
        .map(char::len_utf8)
        .sum();
    if numerals > 0 {
        if let Some(after) = text[numerals..].strip_prefix(['、', '.']) {
            return after.trim_start();
        }
    }
    text
}

/// `P<1-2 digits>` with an optional `级`, at the start of `text`, followed by a
/// boundary. Returns the number and the byte length consumed.
fn p_run(text: &str) -> Option<(u8, usize)> {
    let rest = text.strip_prefix(['P', 'p'])?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=2).contains(&digits) {
        return None;
    }
    let number = rest[..digits].parse().ok()?;
    let mut end = 1 + digits;
    if text[end..].starts_with('级') {
        end += '级'.len_utf8();
    }
    let mut after = text[end..].chars();
    let boundary = match after.next() {
        None => true,
        Some('.') => !after.next().is_some_and(|next| next.is_ascii_digit()),
        Some(next) => !next.is_ascii_alphanumeric() && next != '_',
    };
    boundary.then_some((number, end))
}

/// Separator characters that end a heading's label segment.
fn is_separator(character: char) -> bool {
    matches!(
        character,
        ':' | '·' | '•' | '|' | '/' | ',' | '，' | ';' | '；' | '—' | '–' | '(' | '[' | '【' | '〔'
    )
}

/// Byte offset of the first separator, treating ` - ` (spaced hyphen) as one.
fn first_separator(text: &str) -> Option<usize> {
    text.char_indices()
        .find(|(index, character)| is_separator(*character) || text[*index..].starts_with(" - "))
        .map(|(index, _)| index)
}

/// Byte range of the last separator, treating ` - ` as one.
fn last_separator(text: &str) -> Option<(usize, usize)> {
    text.char_indices().rev().find_map(|(index, character)| {
        if is_separator(character) {
            Some((index, index + character.len_utf8()))
        } else if text[index..].starts_with(" - ") {
            Some((index, index + 3))
        } else {
            None
        }
    })
}

fn head_label(text: &str) -> Option<PriorityLabel> {
    if let Some(open) = text.chars().next().filter(|c| OPEN_BRACKETS.contains(c)) {
        let inner = &text[open.len_utf8()..];
        if let Some(close) = inner.find(closing_bracket(open)) {
            if let Some((label, _)) = whole_label(&inner[..close]) {
                return Some(label);
            }
        }
    }
    if let Some((rank, _)) = p_run(text) {
        return Some(p_label(rank));
    }
    let end = first_separator(text).unwrap_or(text.len());
    if let Some((label, _)) = whole_label(&text[..end]) {
        return Some(label);
    }
    // `Priority: Low` / `优先级：高` — the keyword is its own segment, so the
    // value lives in the next one.
    let keyword = text[..end].trim().to_lowercase();
    if ["priority", "prio", "优先级", "优先"].contains(&keyword.as_str()) && end < text.len() {
        let next = end + text[end..].chars().next().map_or(0, char::len_utf8);
        let value_end = first_separator(&text[next..]).map_or(text.len(), |index| next + index);
        return whole_label(&text[..value_end]).map(|(label, _)| label);
    }
    None
}

fn tail_label(text: &str) -> Option<PriorityLabel> {
    let text = text.trim_end();
    // `… (P0)` / `… 【高优先级】`: brackets make even a bare word a label.
    if let Some(close) = text.chars().last() {
        if let Some(&open) = OPEN_BRACKETS
            .iter()
            .find(|open| closing_bracket(**open) == close)
        {
            let inner_end = text.len() - close.len_utf8();
            if let Some(open_index) = text[..inner_end].rfind(open) {
                if let Some((label, _)) =
                    whole_label(&text[open_index + open.len_utf8()..inner_end])
                {
                    return Some(label);
                }
            }
        }
    }
    // `鉴权 — P1` / `鉴权：高优先级`
    if let Some((_, end)) = last_separator(text) {
        if let Some((label, true)) = whole_label(&text[end..]) {
            return Some(label);
        }
    }
    // `鉴权 P1`
    let word = &text[text
        .rfind(char::is_whitespace)
        .map_or(text.len(), |index| index + 1)..];
    match whole_label(word) {
        Some((label, true)) if word.len() < text.len() => Some(label),
        _ => None,
    }
}

fn p_label(rank: u8) -> PriorityLabel {
    PriorityLabel {
        scale: PriorityScale::PNumber,
        rank,
        raw: format!("P{rank}"),
    }
}

/// The whole candidate must be a label. Returns the label and whether it is
/// *distinctive* (a `P`-number, `!n`, or carrying a priority keyword) rather
/// than a bare word that could be ordinary prose.
fn whole_label(candidate: &str) -> Option<(PriorityLabel, bool)> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return None;
    }
    let lower = candidate.to_lowercase();
    for keyword in ["priority", "prio", "优先级", "优先"] {
        if let Some(rest) = lower.strip_prefix(keyword) {
            let rest = rest
                .trim_start()
                .trim_start_matches([':', '-', '='])
                .trim_start();
            return core_label(rest, true, candidate).map(|label| (label, true));
        }
    }
    for keyword in ["priority", "优先级", "优先", "优"] {
        if let Some(rest) = lower.strip_suffix(keyword) {
            let rest = rest.trim_end();
            return core_label(rest, true, candidate).map(|label| (label, true));
        }
    }
    core_label(&lower, false, candidate).map(|label| {
        let distinctive = label.scale != PriorityScale::Word;
        (label, distinctive)
    })
}

/// Tier of a priority word. Words in the second group are too common in prose
/// (`高`, `normal`) to count without a keyword such as `优先级` / `priority`.
fn word_tier(word: &str, keyword: bool) -> Option<u8> {
    let strong = match word {
        "critical" | "urgent" | "blocker" | "highest" | "紧急" | "特急" | "最高" => Some(0),
        "high" => Some(1),
        "medium" => Some(2),
        "low" | "lowest" | "最低" => Some(3),
        _ => None,
    };
    let weak = || match word {
        "致命" => Some(0),
        "高" => Some(1),
        "中" | "普通" | "一般" | "normal" => Some(2),
        "低" | "minor" => Some(3),
        _ => None,
    };
    strong.or_else(|| keyword.then(weak).flatten())
}

fn core_label(core: &str, keyword: bool, display: &str) -> Option<PriorityLabel> {
    let core = core.trim();
    if let Some((rank, end)) = p_run(core) {
        return (end == core.len()).then(|| p_label(rank));
    }
    if keyword && (1..=2).contains(&core.len()) && core.bytes().all(|byte| byte.is_ascii_digit()) {
        return core.parse().ok().map(p_label);
    }
    if let Some(level) = core
        .strip_prefix('!')
        .and_then(|digits| digits.parse::<u8>().ok())
        .filter(|level| (1..=4).contains(level))
    {
        return Some(PriorityLabel {
            scale: PriorityScale::Native,
            rank: level,
            raw: format!("!{level}"),
        });
    }
    word_tier(core, keyword).map(|rank| PriorityLabel {
        scale: PriorityScale::Word,
        rank,
        raw: display.to_string(),
    })
}
