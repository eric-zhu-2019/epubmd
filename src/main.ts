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
};

const state: ReaderState = { loading: false };
const appElement = document.querySelector<HTMLDivElement>('#app');
if (!appElement) throw new Error('missing #app');
const app: HTMLDivElement = appElement;

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
  document.querySelectorAll<HTMLAnchorElement>('.book-content a[href]').forEach(anchor => {
    anchor.addEventListener('click', event => handleReaderLink(event, anchor));
  });
}

function readerContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  if (state.error) {
    return `<div class="error-state"><strong>Could not open book</strong><span>${escapeHtml(state.error)}</span></div>`;
  }
  if (!book || !chapter) {
    return '<div class="empty-state"><strong>No book loaded</strong><span>Use “Open zip” and choose a zip created by the epubmd CLI.</span></div>';
  }
  const html = renderMarkdownChapter(book, chapter);
  const index = book.chapters.findIndex(item => item.path === chapter.path);
  return `
    <article class="reader-card">
      <style>${scopeBookCss(book.style_css)}</style>
      <div class="book-content">${html}</div>
      <div class="reader-nav">
        <button id="previous-chapter" type="button" ${index <= 0 ? 'disabled' : ''}>Previous</button>
        <button id="next-chapter" type="button" ${index >= book.chapters.length - 1 ? 'disabled' : ''}>Next</button>
      </div>
    </article>
  `;
}

function renderMarkdownChapter(book: BookPayload, chapter: BookChapter): string {
  const rendered = marked.parse(chapter.markdown, { async: false }) as string;
  const clean = DOMPurify.sanitize(rendered, {
    ADD_ATTR: ['target'],
  });
  const template = document.createElement('template');
  template.innerHTML = clean;
  template.content.querySelectorAll<HTMLImageElement>('img[src]').forEach(image => {
    const resolved = resolveBookPath(image.getAttribute('src') ?? '', chapter.path);
    const asset = book.assets.find(candidate => candidate.path === resolved);
    if (asset) image.setAttribute('src', asset.data_url);
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
    state.book = book;
    state.selectedPath = book.chapters[0]?.path;
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
  renderShell();
  if (fragment) {
    requestAnimationFrame(() => document.getElementById(fragment)?.scrollIntoView({ block: 'start' }));
  }
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

renderShell();
