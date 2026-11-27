// 正文里的裸链接识别：`<a>` 之外的文本里，凡是 http/https 前缀的 URL 都切成
// 可点击的锚点。纯文本切分与 DOM 包裹分开，前者便于单测（node --test）。

export interface TextChunk {
  text: string;
  /** 有 href 表示这一段是链接。 */
  href?: string;
}

// 只吃 RFC 3986 的 ASCII 字符：中文正文里链接后面常直接跟「。」或汉字，没有
// 空格可依，因此不能把任意非空白字符都算进 URL。
const URL_PATTERN = /https?:\/\/[A-Za-z0-9\-._~:/?#[\]@!$&'()*+,;=%]+/gi;
// URL 末尾常见的句读/括号：正文里 `见 https://x.com/a。` 不该把「。」算进链接。
const TRAILING_PUNCTUATION = /[.,;:!?、。，；：！？…'"”’)\]}）】」』]+$/;

/** 去掉 URL 末尾的句读；成对括号（URL 自带 `(` 与结尾 `)`）保留。 */
export function trimUrlTrailing(url: string): string {
  let current = url;
  while (TRAILING_PUNCTUATION.test(current)) {
    const trimmed = current.replace(TRAILING_PUNCTUATION, '');
    if (trimmed === current) break;
    const opens = (current.match(/\(/g) ?? []).length;
    const closes = (current.match(/\)/g) ?? []).length;
    if (current.endsWith(')') && opens >= closes) break;
    current = trimmed;
  }
  return current;
}

/**
 * 把一段纯文本切成「文字 / 链接」块。`linkifyBareUrls` 用它驱动 DOM 替换，
 * 单测也直接断言它。
 */
export function splitLinks(text: string): TextChunk[] {
  if (!text) return [];
  const chunks: TextChunk[] = [];
  const pattern = new RegExp(URL_PATTERN.source, 'gi');
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    const url = trimUrlTrailing(match[0]);
    if (!url) continue;
    if (match.index > cursor) chunks.push({ text: text.slice(cursor, match.index) });
    chunks.push({ text: url, href: url });
    cursor = match.index + url.length;
  }
  if (cursor < text.length) chunks.push({ text: text.slice(cursor) });
  return chunks;
}

/**
 * 在已渲染的正文里把裸 URL 包成 `<a>`。跳过已有的链接、代码块与文本域，
 * 避免重复包裹或破坏代码展示。
 */
export function linkifyBareUrls(root: HTMLElement): number {
  const doc = root.ownerDocument;
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(candidate) {
      const node = candidate as Text;
      const parent = node.parentElement;
      if (!parent) return NodeFilter.FILTER_REJECT;
      if (parent.closest('a, code, pre, textarea, script, style')) return NodeFilter.FILTER_REJECT;
      return /https?:\/\//i.test(node.data) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    },
  });
  const targets: Text[] = [];
  while (walker.nextNode()) targets.push(walker.currentNode as Text);

  let wrapped = 0;
  for (const node of targets) {
    const data = node.data;
    const parent = node.parentElement;
    if (!parent) continue;
    const chunks = splitLinks(data);
    if (chunks.length === 1 && !chunks[0].href) continue;
    // 先替换掉该文本节点，再插回切好的片段：整段替换是唯一能保持文档顺序的写法。
    node.data = '';
    for (const chunk of chunks) {
      if (chunk.href) {
        const anchor = doc.createElement('a');
        anchor.href = chunk.href;
        anchor.textContent = chunk.text;
        anchor.rel = 'noopener noreferrer';
        parent.insertBefore(anchor, node);
        wrapped += 1;
      } else if (chunk.text) {
        parent.insertBefore(doc.createTextNode(chunk.text), node);
      }
    }
  }
  return wrapped;
}
