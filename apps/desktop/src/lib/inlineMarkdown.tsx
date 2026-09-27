import * as React from 'react';

/**
 * Renders the inline Markdown a task line can carry.
 *
 * A task body is one line of the source file, so block syntax (headings,
 * lists, fences) cannot occur; only inline spans are rendered: code, strong,
 * emphasis, strikethrough, links and bare URLs. Output is React elements, never
 * HTML strings, so raw `<tags>` in a body stay visible text.
 */
export function renderInlineMarkdown(source: string): React.ReactNode[] {
  let key = 0;
  const nextKey = () => `md${key++}`;
  return parse(source, nextKey);
}

function textOf(node: React.ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(textOf).join('');
  if (React.isValidElement<{ children?: React.ReactNode }>(node)) return textOf(node.props.children);
  return '';
}

/**
 * The same task line as plain text, with Markdown markers removed.
 *
 * For places that cannot hold markup: accessible names, live announcements,
 * and single-line truncated labels. Screen readers otherwise read `**` aloud.
 */
export function plainInlineMarkdown(source: string): string {
  return textOf(renderInlineMarkdown(source));
}

const PUNCTUATION = /[!-/:-@[-`{-~]/;
const WORD = /[A-Za-z0-9]/;
const URL_PATTERN = /https?:\/\/[^\s<>()（）「」，。；、]+[^\s<>()（）「」，。；、.,;:!?'"`*_~]/y;

function safeHref(href: string): string | null {
  const trimmed = href.trim();
  return /^(https?:|mailto:)/i.test(trimmed) ? trimmed : null;
}

/** Index just past a code span that opens at `start`, or -1 when it never closes. */
function codeSpanEnd(source: string, start: number): { run: number; close: number } | null {
  let run = 0;
  while (source[start + run] === '`') run += 1;
  let cursor = start + run;
  while (cursor < source.length) {
    const found = source.indexOf('`', cursor);
    if (found === -1) return null;
    let length = 0;
    while (source[found + length] === '`') length += 1;
    if (length === run) return { run, close: found };
    cursor = found + length;
  }
  return null;
}

/** Finds a closing delimiter, skipping escapes and code spans in between. */
function findClosing(source: string, from: number, delimiter: string): number {
  const single = delimiter.length === 1;
  let index = from;
  while (index < source.length) {
    const character = source[index];
    if (character === '\\') {
      index += 2;
      continue;
    }
    if (character === '`') {
      const span = codeSpanEnd(source, index);
      if (span) {
        index = span.close + span.run;
        continue;
      }
    }
    if (source.startsWith(delimiter, index) && index > from && !/\s/.test(source[index - 1]!)) {
      const doubled = single && source[index + 1] === character;
      const underscoreInWord = character === '_' && WORD.test(source[index + delimiter.length] ?? '');
      if (!doubled && !underscoreInWord) return index;
      if (doubled) {
        index += 2;
        continue;
      }
    }
    index += 1;
  }
  return -1;
}

function parse(source: string, nextKey: () => string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  let text = '';
  const flush = () => {
    if (text) nodes.push(text);
    text = '';
  };
  let index = 0;
  while (index < source.length) {
    const character = source[index]!;

    if (character === '\\' && PUNCTUATION.test(source[index + 1] ?? '')) {
      text += source[index + 1];
      index += 2;
      continue;
    }

    if (character === '`') {
      const span = codeSpanEnd(source, index);
      if (span) {
        let content = source.slice(index + span.run, span.close);
        if (content.length > 2 && content.startsWith(' ') && content.endsWith(' ')) content = content.slice(1, -1);
        flush();
        nodes.push(<code key={nextKey()} className="md-code">{content}</code>);
        index = span.close + span.run;
        continue;
      }
      let run = 0;
      while (source[index + run] === '`') run += 1;
      text += source.slice(index, index + run);
      index += run;
      continue;
    }

    const pair = source.slice(index, index + 2);
    if (pair === '**' || pair === '__' || pair === '~~') {
      const opensInWord = pair === '__' && WORD.test(source[index - 1] ?? '');
      const close = opensInWord || /\s/.test(source[index + 2] ?? ' ') ? -1 : findClosing(source, index + 2, pair);
      if (close !== -1) {
        flush();
        const children = parse(source.slice(index + 2, close), nextKey);
        nodes.push(pair === '~~'
          ? <del key={nextKey()} className="md-del">{children}</del>
          : <strong key={nextKey()} className="md-strong">{children}</strong>);
        index = close + 2;
        continue;
      }
    }

    if (character === '*' || character === '_') {
      const opensInWord = character === '_' && WORD.test(source[index - 1] ?? '');
      const close = opensInWord || /\s/.test(source[index + 1] ?? ' ') ? -1 : findClosing(source, index + 1, character);
      if (close !== -1) {
        flush();
        nodes.push(<em key={nextKey()} className="md-em">{parse(source.slice(index + 1, close), nextKey)}</em>);
        index = close + 1;
        continue;
      }
    }

    if (character === '[') {
      const labelEnd = findClosing(source, index + 1, ']');
      if (labelEnd !== -1 && source[labelEnd + 1] === '(') {
        const hrefEnd = source.indexOf(')', labelEnd + 2);
        const href = hrefEnd === -1 ? null : safeHref(source.slice(labelEnd + 2, hrefEnd));
        if (href) {
          flush();
          nodes.push(
            <a key={nextKey()} className="md-link" href={href} target="_blank" rel="noreferrer noopener">
              {parse(source.slice(index + 1, labelEnd), nextKey)}
            </a>,
          );
          index = hrefEnd + 1;
          continue;
        }
      }
    }

    if ((character === 'h' || character === 'H') && !WORD.test(source[index - 1] ?? '')) {
      URL_PATTERN.lastIndex = index;
      const match = URL_PATTERN.exec(source);
      if (match) {
        flush();
        nodes.push(
          <a key={nextKey()} className="md-link" href={match[0]} target="_blank" rel="noreferrer noopener">
            {match[0]}
          </a>,
        );
        index += match[0].length;
        continue;
      }
    }

    text += character;
    index += 1;
  }
  flush();
  return nodes;
}
