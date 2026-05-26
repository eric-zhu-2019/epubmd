import { structureFlatTocMarkdown } from './tocStructure.js';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

const linked = structureFlatTocMarkdown(`# Contents

1 [Introduction to Fennel and Lua](chapter-1.md) 1.1 [Overview of Lua Language](chapter-1.md#overview) 1.2 [Fennel as a Lisp for Lua](chapter-1.md#fennel) 2 [Setting Up Your Fennel Environment](chapter-2.md)`);
assert(linked.includes('# Contents'), 'keeps existing Contents heading');
assert(linked.includes('- 1 [Introduction to Fennel and Lua](chapter-1.md)'), 'splits first linked entry');
assert(linked.includes('- 1.1 [Overview of Lua Language](chapter-1.md#overview)'), 'splits nested linked entry');
assert(!linked.includes('Lua 1.1 [Overview'), 'removes one-line TOC run-on text');

const inline = structureFlatTocMarkdown('Contents 1 Introduction to Fennel and Lua 1.1 Overview of Lua Language 1.2 Fennel as a Lisp for Lua 2 Setting Up Your Fennel Environment');
assert(inline.includes('## Contents'), 'promotes inline Contents label to heading');
assert(inline.includes('- 1.2 Fennel as a Lisp for Lua'), 'splits plain text TOC entries');

const singleNewline = structureFlatTocMarkdown(`# Contents
1 [Introduction to Fennel and Lua](chapter-1.md) 1.1 [Overview of Lua Language](chapter-1.md#overview) 1.2 [Fennel as a Lisp for Lua](chapter-1.md#fennel) 2 [Setting Up Your Fennel Environment](chapter-2.md)`);
assert(singleNewline.includes('# Contents\n\n- 1 [Introduction to Fennel and Lua](chapter-1.md)'), 'splits TOC when heading and body share a Markdown block');

const htmlParagraph = structureFlatTocMarkdown(`# Contents
<p>1 <a href="chapter-1.md">Introduction to Fennel and Lua</a> 1.1 <a href="chapter-1.md#overview">Overview of Lua Language</a> 1.2 <a href="chapter-1.md#fennel">Fennel as a Lisp for Lua</a> 2 <a href="chapter-2.md">Setting Up Your Fennel Environment</a></p>`);
assert(htmlParagraph.includes('- 1 <a href="chapter-1.md">Introduction to Fennel and Lua</a>'), 'unwraps raw HTML paragraph TOC entries');
assert(!htmlParagraph.includes('- <p>1'), 'does not keep paragraph tag inside first list item');

const koboAnchors = structureFlatTocMarkdown(`<a id="book-columns"></a>
<a id="book-inner"></a>
<a id="contents"></a>
## <a id="x2-1000"></a> <a id="kobo.1.1"></a> Contents

<a id="kobo.2.1"></a> 1 <a id="QQ2-4-3"></a> [<a id="kobo.3.1"></a> Introduction to Fennel and Lua](004-Chapter-1.md) <a id="kobo.4.1"></a> 1.1 <a id="QQ2-4-4"></a> [<a id="kobo.5.1"></a> Overview of Lua Language](004-Chapter-1.md#overview) <a id="kobo.6.1"></a> 2 <a id="QQ2-5-9"></a> [<a id="kobo.7.1"></a> Setting Up Your Fennel Environment](005-Chapter-2.md)`);
assert(koboAnchors.includes('- 1 <a id="QQ2-4-3"></a>'), 'splits TOC entries when anchor-only lines precede the Contents heading');
assert(koboAnchors.includes('- 1.1 <a id="QQ2-4-4"></a>'), 'keeps subsection TOC entries on separate list lines with inline anchors');
assert(!koboAnchors.includes('Lua](004-Chapter-1.md) <a id="kobo.4.1"></a> 1.1'), 'removes Kobo run-on TOC text');
assert(!koboAnchors.includes('[<a id="kobo.3.1"></a> Introduction'), 'moves Kobo anchors outside Markdown link labels');
assert(koboAnchors.includes('<a id="kobo.3.1"></a> [Introduction to Fennel and Lua](004-Chapter-1.md)'), 'keeps TOC link labels clickable after moving anchors');

const ordinary = structureFlatTocMarkdown('Version 1 has notes 1.1 but this is not a Contents block.');
assert(ordinary === 'Version 1 has notes 1.1 but this is not a Contents block.', 'does not alter ordinary paragraphs');
