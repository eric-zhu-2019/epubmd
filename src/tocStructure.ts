export function structureFlatTocMarkdown(markdown: string): string {
  const blocks = markdown.split(/\n{2,}/);
  const structured: string[] = [];
  let previousWasTocHeading = false;

  for (const block of blocks) {
    const trimmed = block.trim();
    const headingWithBody = tocHeadingBlockAndBody(trimmed);
    if (headingWithBody) {
      const entries = splitFlatTocEntries(headingWithBody.body);
      structured.push(entries.length >= 3 ? `${headingWithBody.heading}\n\n${formatTocEntries(entries)}` : block);
      previousWasTocHeading = false;
      continue;
    }

    const inlineToc = flatTocHeadingAndBody(trimmed);
    if (inlineToc) {
      const entries = splitFlatTocEntries(inlineToc.body);
      structured.push(entries.length >= 3 ? `## ${inlineToc.heading}\n\n${formatTocEntries(entries)}` : block);
      previousWasTocHeading = false;
      continue;
    }

    if (previousWasTocHeading) {
      const entries = splitFlatTocEntries(trimmed);
      if (entries.length >= 3) {
        structured.push(formatTocEntries(entries));
        previousWasTocHeading = false;
        continue;
      }
    }

    structured.push(block);
    previousWasTocHeading = isTocHeadingBlock(trimmed);
  }

  return structured.join('\n\n');
}

type InlineToc = {
  heading: 'Contents' | 'Table of Contents';
  body: string;
};

function tocHeadingBlockAndBody(block: string): { heading: string; body: string } | undefined {
  const [firstLine, ...rest] = block.split(/\n/);
  if (!firstLine || rest.length === 0 || !isTocHeadingBlock(firstLine)) return undefined;
  return { heading: firstLine.trim(), body: unwrapParagraph(rest.join('\n').trim()) };
}

function flatTocHeadingAndBody(block: string): InlineToc | undefined {
  const lower = block.toLowerCase();
  if (lower === 'contents' || lower === 'table of contents') return undefined;
  if (lower.startsWith('table of contents ')) {
    return { heading: 'Table of Contents', body: unwrapParagraph(block.slice('table of contents'.length).trim()) };
  }
  if (lower.startsWith('contents ')) {
    return { heading: 'Contents', body: unwrapParagraph(block.slice('contents'.length).trim()) };
  }
  return undefined;
}

function isTocHeadingBlock(block: string): boolean {
  const text = lastNonEmptyLine(block)
    .replace(anchorOnlyLinePattern(), '')
    .replace(/^#+\s+/, '')
    .replace(/<[^>]+>/g, '')
    .trim()
    .toLowerCase();
  return text === 'contents' || text === 'table of contents';
}

function lastNonEmptyLine(block: string): string {
  return block.split(/\n/).map(line => line.trim()).filter(Boolean).at(-1) ?? block;
}

function formatTocEntries(entries: string[]): string {
  return entries.map(entry => `- ${normalizeTocEntry(entry)}`).join('\n');
}

function normalizeTocEntry(entry: string): string {
  return entry.replace(/\[((?:\s*<a\s+id="[^"]+"><\/a>)+\s*)([^\]]+?)\]\(([^)]+)\)/gi, (_, anchors: string, label: string, href: string) => {
    return `${anchors.trim()} [${label.trim()}](${href})`;
  });
}

function splitFlatTocEntries(body: string): string[] {
  const normalizedBody = unwrapParagraph(body).replace(/&nbsp;/gi, ' ');
  const starts = markerStarts(normalizedBody);
  if (starts.length < 3) return [];
  return starts
    .map((start, index) => normalizedBody.slice(start, starts[index + 1] ?? normalizedBody.length).trim())
    .filter(entry => entry.length > 0);
}

function unwrapParagraph(value: string): string {
  return value.replace(/^<p(?:\s[^>]*)?>([\s\S]*)<\/p>$/i, '$1').trim();
}

function anchorOnlyLinePattern(): RegExp {
  return /^(?:<a\s+id="[^"]+"><\/a>\s*)+/i;
}

function markerStarts(body: string): number[] {
  const starts: number[] = [];
  for (let index = 0; index < body.length; index += 1) {
    if (isAsciiDigit(body[index]) && isMarkerAt(body, index)) starts.push(index);
  }
  return starts;
}

function isMarkerAt(body: string, index: number): boolean {
  if (index > 0 && !/\s/.test(body[index - 1])) return false;
  let cursor = index;
  let sawDigit = false;
  while (cursor < body.length && isAsciiDigit(body[cursor])) {
    sawDigit = true;
    cursor += 1;
  }
  if (!sawDigit) return false;

  while (body[cursor] === '.') {
    cursor += 1;
    const groupStart = cursor;
    while (cursor < body.length && isAsciiDigit(body[cursor])) cursor += 1;
    if (groupStart === cursor) return false;
  }

  return cursor < body.length && /\s/.test(body[cursor]);
}

function isAsciiDigit(value: string | undefined): boolean {
  return value !== undefined && value >= '0' && value <= '9';
}
