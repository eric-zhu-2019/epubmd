import { scopeReaderCss, styleTagContent } from './cssScope.js';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

const scoped = scopeReaderCss(`
@charset "utf-8";
html body { color: #111; }
.typora-export #write h1, #write p { line-height: 1.6; }
html .typora-export blockquote { color: #555; }
@media screen { body.typora-export a:hover { color: red; } }
`);

assert(scoped.includes('.book-content{ color: #111; }'), 'scopes html body selector');
assert(
  scoped.includes('.book-content h1, .book-content p{ line-height: 1.6; }'),
  'scopes #write selectors with Typora prefixes',
);
assert(
  scoped.includes('.book-content blockquote{ color: #555; }'),
  'scopes html .typora-export selector',
);
assert(
  scoped.includes('@media screen{.book-content a:hover{ color: red; } }'),
  'scopes selectors inside media rules',
);
assert(!scoped.includes('#write'), 'removes raw #write selector');
assert(!scoped.includes('html body'), 'removes raw html body selector');
assert(styleTagContent('</style>').includes('<\\/style>'), 'escapes closing style tags');
