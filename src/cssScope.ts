export function scopeReaderCss(css: string): string {
  if (!css.trim()) return '';
  return scopeCssRules(css.replace(/@charset\s+["'][^"']+["'];?/gi, ''), '.book-content');
}

function scopeCssRules(css: string, scope: string): string {
  let output = '';
  let index = 0;
  while (index < css.length) {
    const open = css.indexOf('{', index);
    if (open === -1) {
      output += css.slice(index);
      break;
    }
    const selector = css.slice(index, open).trim();
    const close = matchingBraceIndex(css, open);
    if (close === -1) {
      output += css.slice(index);
      break;
    }
    const body = css.slice(open + 1, close);
    if (selector.startsWith('@media') || selector.startsWith('@supports')) {
      output += `${selector}{${scopeCssRules(body, scope)}}`;
    } else if (selector.startsWith('@')) {
      output += `${selector}{${body}}`;
    } else {
      output += `${scopeSelectors(selector, scope)}{${body}}`;
    }
    index = close + 1;
  }
  return output;
}

function matchingBraceIndex(css: string, openIndex: number): number {
  let depth = 0;
  let quote: string | undefined;
  for (let index = openIndex; index < css.length; index += 1) {
    const character = css[index];
    const previous = css[index - 1];
    if (quote) {
      if (character === quote && previous !== '\\') quote = undefined;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    if (character === '{') depth += 1;
    if (character === '}') {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function scopeSelectors(selectorText: string, scope: string): string {
  return selectorText
    .split(',')
    .map(selector => scopeSelector(selector.trim(), scope))
    .join(', ');
}

function scopeSelector(selector: string, scope: string): string {
  if (!selector || selector.startsWith(scope) || selector.startsWith('.reader-card')) return selector;
  if (selector.startsWith(':root')) return selector.replace(/^:root\b/, '.reader-card');

  const writeIndex = selector.lastIndexOf('#write');
  if (writeIndex >= 0) {
    return `${scope}${selector.slice(writeIndex + '#write'.length)}`;
  }

  let value = selector
    .replace(/^html\b(?:\.[\w-]+)?\s+body\b(?:\.[\w-]+)?/, scope)
    .replace(/^html\b(?:\.[\w-]+)?\s+\.typora-export\b/, scope)
    .replace(/^html\b(?:\.[\w-]+)?/, scope)
    .replace(/^body\b(?:\.[\w-]+)?/, scope)
    .replace(/^\.typora-export\b/, scope);

  if (value.startsWith(scope) || value.startsWith('.reader-card')) return value;
  return value.startsWith(':') ? `${scope}${value}` : `${scope} ${value}`;
}

export function styleTagContent(css: string): string {
  return css.replace(/<\/style/gi, '<\\/style');
}
