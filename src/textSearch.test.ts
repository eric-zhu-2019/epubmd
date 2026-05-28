import { findTextMatches, searchableMarkdownText } from './textSearch.js';

const chapters = [
  {
    path: 'chapters/001.md',
    title: 'Intro',
    markdown: '# Intro\n\nThis book explains Lisp and [Lisp macros](002.md#macros).',
  },
  {
    path: 'chapters/002.md',
    title: 'Macros',
    markdown: '# Macros\n\nCode is data. Lisp treats code as lists.',
  },
];

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const text = searchableMarkdownText('Read [the chapter](chapter.md#target) and `code`.');
assert(text.includes('the chapter'), 'link label should remain searchable');
assert(!text.includes('chapter.md'), 'link destination should not pollute search text');

const matches = findTextMatches(chapters, 'lisp');
assert(matches.length === 3, `expected 3 matches, got ${matches.length}`);
assert(matches[0].chapterPath === 'chapters/001.md', 'first match should be in first chapter');
assert(matches[0].matchIndexInChapter === 0, 'first chapter index should start at 0');
assert(matches[1].matchIndexInChapter === 1, 'second first-chapter match should increment');
assert(matches[2].chapterPath === 'chapters/002.md', 'third match should be in second chapter');
assert(matches[2].globalIndex === 2, 'global index should follow book order');
assert(findTextMatches(chapters, '   ').length === 0, 'blank query should not match');
