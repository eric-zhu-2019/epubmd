export type ChapterSection = {
  id: string;
  title: string;
  level: number;
};

const headingPattern = /^(#{1,6})\s+(.+?)\s*#*\s*$/;
const anchorPattern = /^<a\s+[^>]*id=["']([^"']+)["'][^>]*><\/a>\s*$/i;

export function extractChapterSections(markdown: string): ChapterSection[] {
  const sections: ChapterSection[] = [];
  const usedIds = new Map<string, number>();
  let pendingAnchor: string | undefined;
  let fenceMarker: string | undefined;

  for (const rawLine of markdown.split('\n')) {
    const line = rawLine.trim();
    const fenceMatch = line.match(/^(```+|~~~~+)/);
    if (fenceMatch) {
      fenceMarker = fenceMarker ? undefined : fenceMatch[1][0];
      continue;
    }
    if (fenceMarker) continue;

    const anchorMatch = line.match(anchorPattern);
    if (anchorMatch) {
      pendingAnchor = anchorMatch[1];
      continue;
    }

    const headingMatch = line.match(headingPattern);
    if (!headingMatch) {
      if (line) pendingAnchor = undefined;
      continue;
    }

    const title = stripInlineMarkdown(headingMatch[2]);
    if (!title) continue;
    const baseId = pendingAnchor || slugifyHeading(title);
    sections.push({
      id: uniqueId(baseId, usedIds),
      title,
      level: headingMatch[1].length,
    });
    pendingAnchor = undefined;
  }

  return sections;
}

function uniqueId(baseId: string, usedIds: Map<string, number>): string {
  const safeBase = baseId || 'section';
  const count = usedIds.get(safeBase) ?? 0;
  usedIds.set(safeBase, count + 1);
  return count === 0 ? safeBase : `${safeBase}-${count + 1}`;
}

function slugifyHeading(value: string): string {
  const slug = value
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^\p{Letter}\p{Number}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'section';
}

function stripInlineMarkdown(value: string): string {
  return value
    .replace(/<[^>]+>/g, '')
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/[`*_~]+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}
