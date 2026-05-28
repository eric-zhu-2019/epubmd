export type SearchableChapter = {
  path: string;
  title: string;
  markdown: string;
};

export type TextSearchMatch = {
  chapterPath: string;
  chapterTitle: string;
  matchIndexInChapter: number;
  globalIndex: number;
  excerpt: string;
};

const excerptRadius = 44;

export function findTextMatches(chapters: SearchableChapter[], query: string): TextSearchMatch[] {
  const needle = normalizeQuery(query);
  if (!needle) return [];

  const matches: TextSearchMatch[] = [];
  for (const chapter of chapters) {
    const text = searchableMarkdownText(chapter.markdown);
    const haystack = text.toLocaleLowerCase();
    let offset = 0;
    let matchIndexInChapter = 0;
    while (offset < haystack.length) {
      const found = haystack.indexOf(needle, offset);
      if (found === -1) break;
      matches.push({
        chapterPath: chapter.path,
        chapterTitle: chapter.title,
        matchIndexInChapter,
        globalIndex: matches.length,
        excerpt: searchExcerpt(text, found, needle.length),
      });
      matchIndexInChapter += 1;
      offset = found + Math.max(needle.length, 1);
    }
  }
  return matches;
}

export function normalizeQuery(query: string): string {
  return query.trim().replace(/\s+/g, ' ').toLocaleLowerCase();
}

export function searchableMarkdownText(markdown: string): string {
  return markdown
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, ' ')
    .replace(/[`*_~>#|]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export function searchExcerpt(text: string, start: number, length: number): string {
  const rawStart = Math.max(0, start - excerptRadius);
  const rawEnd = Math.min(text.length, start + length + excerptRadius);
  const prefix = rawStart > 0 ? '…' : '';
  const suffix = rawEnd < text.length ? '…' : '';
  return `${prefix}${text.slice(rawStart, rawEnd).replace(/\s+/g, ' ').trim()}${suffix}`;
}
