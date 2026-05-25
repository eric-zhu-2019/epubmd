import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import './styles.css';

type BookChapter = {
  path: string;
  title: string;
  markdown: string;
};

type BookAsset = {
  path: string;
  data_url: string;
};

type BookPayload = {
  title: string;
  readme: string;
  style_css: string;
  chapters: BookChapter[];
  assets: BookAsset[];
};

type ReaderState = {
  book?: BookPayload;
  selectedPath?: string;
  error?: string;
  loading: boolean;
  pendingFragment?: string;
  renderToken: number;
};

const state: ReaderState = { loading: false, renderToken: 0 };
const appElement = document.querySelector<HTMLDivElement>('#app');
if (!appElement) throw new Error('missing #app');
const app: HTMLDivElement = appElement;
const renderedChapterCache = new Map<string, string>();
const assetIndexCache = new WeakMap<BookPayload, Map<string, string>>();

marked.use({
  gfm: true,
  breaks: false,
});

function renderShell(): void {
  const book = state.book;
  const current = currentChapter();
  app.innerHTML = `
    <main class="shell">
      <aside class="sidebar">
        <div class="brand">
          <h1>epubmd</h1>
          <button id="open-book" type="button">${state.loading ? 'Opening…' : 'Open zip'}</button>
        </div>
        <p class="book-title">${escapeHtml(book?.title ?? 'Open an epubmd zip archive to start reading.')}</p>
        <nav class="chapter-list" aria-label="Chapters">
          ${(book?.chapters ?? []).map(chapter => `
            <button class="chapter-button ${chapter.path === state.selectedPath ? 'active' : ''}" type="button" data-chapter="${escapeHtml(chapter.path)}">
              ${escapeHtml(chapter.title)}
            </button>
          `).join('')}
        </nav>
      </aside>
      <section class="reader-pane">
        ${readerContent(book, current)}
      </section>
    </main>
  `;
  document.querySelector<HTMLButtonElement>('#open-book')?.addEventListener('click', openBook);
  document.querySelectorAll<HTMLButtonElement>('[data-chapter]').forEach(button => {
    button.addEventListener('click', () => {
      state.selectedPath = button.dataset.chapter;
      state.error = undefined;
      renderShell();
    });
  });
  document.querySelector<HTMLButtonElement>('#previous-chapter')?.addEventListener('click', () => moveChapter(-1));
  document.querySelector<HTMLButtonElement>('#next-chapter')?.addEventListener('click', () => moveChapter(1));
  bindReaderLinks();
  void renderSelectedChapter(book, current);
}

function readerContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  if (state.error) {
    return `<div class="error-state"><strong>Could not open book</strong><span>${escapeHtml(state.error)}</span></div>`;
  }
  if (!book || !chapter) {
    return '<div class="empty-state"><strong>No book loaded</strong><span>Use “Open zip” and choose a zip created by the epubmd CLI.</span></div>';
  }
  const index = book.chapters.findIndex(item => item.path === chapter.path);
  const cached = renderedChapterCache.get(chapterCacheKey(book, chapter));
  return `
    <article class="reader-card">
      <style>${scopeBookCss(book.style_css)}</style>
      <div class="book-content" data-render-chapter="${escapeHtml(chapter.path)}">${cached ?? loadingChapterMarkup(chapter)}</div>
      <div class="reader-nav">
        <button id="previous-chapter" type="button" ${index <= 0 ? 'disabled' : ''}>Previous</button>
        <button id="next-chapter" type="button" ${index >= book.chapters.length - 1 ? 'disabled' : ''}>Next</button>
      </div>
    </article>
  `;
}

async function renderSelectedChapter(book: BookPayload | undefined, chapter: BookChapter | undefined): Promise<void> {
  if (!book || !chapter) return;
  const target = document.querySelector<HTMLDivElement>(`.book-content[data-render-chapter="${cssString(chapter.path)}"]`);
  if (!target) return;
  const token = ++state.renderToken;
  const key = chapterCacheKey(book, chapter);
  const cached = renderedChapterCache.get(key);
  if (cached) {
    target.innerHTML = cached;
    bindReaderLinks(target);
    scrollPendingFragment();
    return;
  }

  target.innerHTML = loadingChapterMarkup(chapter);
  await nextFrame();
  if (token !== state.renderToken) return;

  const chunks = splitMarkdownForProgressiveRender(chapter.markdown);
  const renderedChunks: string[] = [];
  target.innerHTML = '';

  for (let index = 0; index < chunks.length; index += 1) {
    await nextFrame();
    if (token !== state.renderToken) return;
    const html = renderMarkdownFragment(book, chapter, chunks[index]);
    renderedChunks.push(html);
    target.insertAdjacentHTML('beforeend', html);
    scrollPendingFragment();
  }

  if (token !== state.renderToken) return;
  const html = renderedChunks.join('');
  renderedChapterCache.set(key, html);
  bindReaderLinks(target);
  scrollPendingFragment();
}

function renderMarkdownFragment(book: BookPayload, chapter: BookChapter, markdown: string): string {
  const rendered = marked.parse(markdown, { async: false }) as string;
  const clean = DOMPurify.sanitize(rendered, {
    ADD_ATTR: ['target'],
  });
  const template = document.createElement('template');
  template.innerHTML = clean;
  template.content.querySelectorAll<HTMLImageElement>('img[src]').forEach(image => {
    const resolved = resolveBookPath(image.getAttribute('src') ?? '', chapter.path);
    const dataUrl = assetIndex(book).get(resolved);
    if (dataUrl) image.setAttribute('src', dataUrl);
  });
  template.content.querySelectorAll<HTMLAnchorElement>('a[href]').forEach(anchor => {
    const href = anchor.getAttribute('href') ?? '';
    if (/^https?:\/\//i.test(href)) {
      anchor.setAttribute('target', '_blank');
      anchor.setAttribute('rel', 'noreferrer');
    }
  });
  return template.innerHTML;
}

async function openBook(): Promise<void> {
  state.loading = true;
  state.error = undefined;
  renderShell();
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'epubmd zip', extensions: ['zip'] }],
    });
    if (typeof selected !== 'string') return;
    const book = await invoke<BookPayload>('load_book_zip', { path: selected });
    renderedChapterCache.clear();
    state.book = book;
    state.selectedPath = book.chapters[0]?.path;
    state.pendingFragment = undefined;
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    renderShell();
  }
}

function currentChapter(): BookChapter | undefined {
  return state.book?.chapters.find(chapter => chapter.path === state.selectedPath) ?? state.book?.chapters[0];
}

function moveChapter(offset: number): void {
  const chapters = state.book?.chapters ?? [];
  const index = chapters.findIndex(chapter => chapter.path === state.selectedPath);
  const next = chapters[index + offset];
  if (!next) return;
  state.selectedPath = next.path;
  state.pendingFragment = undefined;
  renderShell();
}

function handleReaderLink(event: MouseEvent, anchor: HTMLAnchorElement): void {
  const href = anchor.getAttribute('href') ?? '';
  if (/^https?:\/\//i.test(href)) return;
  const [targetPath, fragment] = href.split('#');
  const current = currentChapter();
  const resolved = targetPath ? resolveBookPath(targetPath, current?.path ?? '') : current?.path;
  const target = state.book?.chapters.find(chapter => chapter.path === resolved);
  if (!target) return;
  event.preventDefault();
  state.selectedPath = target.path;
  state.pendingFragment = fragment;
  renderShell();
}

function bindReaderLinks(scope: ParentNode = document): void {
  const anchors = scope === document
    ? scope.querySelectorAll<HTMLAnchorElement>('.book-content a[href]')
    : scope.querySelectorAll<HTMLAnchorElement>('a[href]');
  anchors.forEach(anchor => {
    if (anchor.dataset.bound === 'true') return;
    anchor.dataset.bound = 'true';
    anchor.addEventListener('click', event => handleReaderLink(event, anchor));
  });
}

function splitMarkdownForProgressiveRender(markdown: string): string[] {
  const maxChunkLength = 60_000;
  const chunks: string[] = [];
  const current: string[] = [];
  let currentLength = 0;
  let fence: string | undefined;

  for (const line of markdown.split('\n')) {
    const fenceMatch = line.match(/^(```+|~~~~+)/);
    if (fenceMatch) {
      fence = fence ? undefined : fenceMatch[1][0];
    }
    current.push(line);
    currentLength += line.length + 1;
    if (!fence && currentLength >= maxChunkLength && line.trim() === '') {
      chunks.push(current.join('\n'));
      current.length = 0;
      currentLength = 0;
    }
  }

  if (current.length > 0) chunks.push(current.join('\n'));
  return chunks.length > 0 ? chunks : [markdown];
}

function loadingChapterMarkup(chapter: BookChapter): string {
  return `
    <div class="chapter-loading" role="status" aria-live="polite">
      <strong>${escapeHtml(chapter.title)}</strong>
      <span>Rendering chapter…</span>
    </div>
  `;
}

function nextFrame(): Promise<void> {
  return new Promise(resolve => requestAnimationFrame(() => resolve()));
}

function chapterCacheKey(book: BookPayload, chapter: BookChapter): string {
  return `${book.title}\u0000${chapter.path}\u0000${chapter.markdown.length}`;
}

function assetIndex(book: BookPayload): Map<string, string> {
  const cached = assetIndexCache.get(book);
  if (cached) return cached;
  const index = new Map(book.assets.map(asset => [asset.path, asset.data_url]));
  assetIndexCache.set(book, index);
  return index;
}

function scrollPendingFragment(): void {
  const fragment = state.pendingFragment;
  if (!fragment) return;
  const target = document.getElementById(fragment);
  if (!target) return;
  target.scrollIntoView({ block: 'start' });
  state.pendingFragment = undefined;
}

function resolveBookPath(reference: string, basePath: string): string {
  if (!reference || /^([a-z]+:)?\/\//i.test(reference) || reference.startsWith('data:')) return reference;
  const [pathOnly] = reference.split('#');
  const baseParts = basePath.split('/');
  baseParts.pop();
  const parts = pathOnly.split('/');
  const stack = pathOnly.startsWith('/') ? [] : baseParts;
  for (const part of parts) {
    if (!part || part === '.') continue;
    if (part === '..') stack.pop();
    else stack.push(part);
  }
  return stack.join('/');
}

function scopeBookCss(css: string): string {
  if (!css.trim()) return '';
  return css
    .replace(/(^|}|\s)body\s*{/g, '$1.book-content {')
    .replace(/(^|}|\s)body\s*,/g, '$1.book-content,');
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"]/g, character => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
  }[character] ?? character));
}

function cssString(value: string): string {
  return window.CSS?.escape ? window.CSS.escape(value) : value.replace(/["\\]/g, '\\$&');
}

renderShell();
