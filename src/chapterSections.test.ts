import { extractChapterSections } from './chapterSections.js';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

const sections = extractChapterSections(`
<a id="intro"></a>
# Introduction

Some text.

## 1.1 Overview of Lua Language

\`\`\`
# Not a section
\`\`\`

## 1.1 Overview of Lua Language
`);

assert(sections.length === 3, 'extracts headings outside fenced code');
assert(sections[0].id === 'intro', 'uses preceding anchor as section id');
assert(sections[0].level === 1, 'tracks heading level');
assert(sections[1].id === '1-1-overview-of-lua-language', 'slugifies plain headings');
assert(sections[2].id === '1-1-overview-of-lua-language-2', 'deduplicates repeated headings');
assert(sections[1].title === '1.1 Overview of Lua Language', 'keeps clean heading title');
