import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { scopeReaderCss, styleTagContent } from './cssScope';
import { interpretHorizontalSwipe, interpretHorizontalWheel, targetChapterPath, type ChapterOffset, type GesturePoint } from './readerNavigation';
import './styles.css';

type LibraryBook = {
  path: string;
  file_name: string;
  title: string;
  chapter_count: number;
  modified_ms: number;
};

type ThemeEntry = {
  path: string;
  file_name: string;
  name: string;
};

type AppPaths = {
  books_dir: string;
  themes_dir: string;
};

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

type ThemePayload = {
  name: string;
  css: string;
};

type ImportPayload = {
  book: LibraryBook;
  payload: BookPayload;
};

type ReaderState = {
  appPaths?: AppPaths;
  library: LibraryBook[];
  themes: ThemeEntry[];
  book?: BookPayload;
  selectedBookPath?: string;
  selectedPath?: string;
  error?: string;
  libraryError?: string;
  themeError?: string;
  loading: boolean;
  importing: boolean;
  libraryLoading: boolean;
  themeLoading: boolean;
  themeCss?: string;
  themeName?: string;
  themePath?: string;
  pendingFragment?: string;
  renderToken: number;
};

const state: ReaderState = {
  library: [],
  themes: [],
  loading: false,
  importing: false,
  libraryLoading: true,
  themeLoading: true,
  renderToken: 0,
};
const appElement = document.querySelector<HTMLDivElement>('#app');
if (!appElement) throw new Error('missing #app');
const app: HTMLDivElement = appElement;
const renderedChapterCache = new Map<string, string>();
const assetIndexCache = new WeakMap<BookPayload, Map<string, string>>();
let swipeStart: GesturePoint | undefined;
let suppressNextReaderClickUntil = 0;
let accumulatedHorizontalWheelDelta = 0;
let wheelResetTimer: number | undefined;
let lastWheelNavigationAt = 0;

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
          <div>
            <h1>epubmd</h1>
            <span class="brand-subtitle">Markdown e-reader</span>
          </div>
          <button id="import-epub" type="button" ${state.importing || state.loading ? 'disabled' : ''}>${state.importing ? 'Importing…' : 'Import EPUB'}</button>
        </div>

        <section class="sidebar-section library-panel" aria-label="Library">
          <div class="section-heading">
            <span>Library</span>
            <button id="refresh-library" class="link-button" type="button">Refresh</button>
          </div>
          <p class="folder-hint">${escapeHtml(state.appPaths?.books_dir ?? '~/.config/epubmd/books')}</p>
          ${state.libraryError ? `<p class="inline-error">${escapeHtml(state.libraryError)}</p>` : ''}
          ${libraryContent()}
        </section>

        <section class="sidebar-section theme-panel" aria-label="Theme controls">
          <div class="section-heading">
            <span>Theme</span>
            <button id="refresh-themes" class="link-button" type="button">Refresh</button>
          </div>
          <p class="folder-hint">${escapeHtml(state.appPaths?.themes_dir ?? '~/.config/epubmd/themes')}</p>
          <select id="theme-select" class="theme-select" ${state.themeLoading ? 'disabled' : ''}>
            <option value="">Default archive style</option>
            ${state.themes.map(theme => `
              <option value="${escapeHtml(theme.path)}" ${theme.path === state.themePath ? 'selected' : ''}>${escapeHtml(theme.name)}</option>
            `).join('')}
          </select>
          ${state.themeName ? `<span class="theme-name">Using ${escapeHtml(state.themeName)}</span>` : ''}
          ${state.themeError ? `<span class="theme-error">${escapeHtml(state.themeError)}</span>` : ''}
        </section>

        <section class="sidebar-section chapter-panel" aria-label="Chapters">
          <div class="section-heading"><span>Chapters</span></div>
          <p class="book-title">${escapeHtml(book?.title ?? 'Select a book to start reading.')}</p>
          <nav class="chapter-list" aria-label="Chapters">
            ${(book?.chapters ?? []).map(chapter => `
              <button class="chapter-button ${chapter.path === state.selectedPath ? 'active' : ''}" type="button" data-chapter="${escapeHtml(chapter.path)}">
                ${escapeHtml(chapter.title)}
              </button>
            `).join('')}
          </nav>
        </section>
      </aside>
      <section class="reader-pane">
        ${readerContent(book, current)}
      </section>
    </main>
  `;
  document.querySelector<HTMLButtonElement>('#import-epub')?.addEventListener('click', importEpub);
  document.querySelector<HTMLButtonElement>('#refresh-library')?.addEventListener('click', () => void refreshLibrary());
  document.querySelector<HTMLButtonElement>('#refresh-themes')?.addEventListener('click', () => void refreshThemes());
  document.querySelector<HTMLSelectElement>('#theme-select')?.addEventListener('change', event => {
    const select = event.currentTarget as HTMLSelectElement;
    void selectTheme(select.value);
  });
  document.querySelectorAll<HTMLButtonElement>('[data-book]').forEach(button => {
    button.addEventListener('click', () => {
      if (button.dataset.book) void openLibraryBook(button.dataset.book);
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-chapter]').forEach(button => {
    button.addEventListener('click', () => {
      state.selectedPath = button.dataset.chapter;
      state.error = undefined;
      state.pendingFragment = undefined;
      renderShell();
    });
  });
  document.querySelector<HTMLButtonElement>('#previous-chapter')?.addEventListener('click', () => moveChapter(-1));
  document.querySelector<HTMLButtonElement>('#next-chapter')?.addEventListener('click', () => moveChapter(1));
  bindReaderGestures();
  bindReaderLinks();
  void renderSelectedChapter(book, current);
}

function libraryContent(): string {
  if (state.libraryLoading) {
    return '<div class="library-empty">Loading books…</div>';
  }
  if (state.library.length === 0) {
    return '<div class="library-empty">No .zmd books yet. Import an EPUB to add it here.</div>';
  }
  return `
    <div class="book-list">
      ${state.library.map(book => `
        <button class="book-button ${book.path === state.selectedBookPath ? 'active' : ''}" type="button" data-book="${escapeHtml(book.path)}" ${state.loading ? 'disabled' : ''}>
          <span class="book-button-title">${escapeHtml(state.loading && book.path === state.selectedBookPath ? 'Opening…' : book.title)}</span>
          <span class="book-button-meta">${escapeHtml(book.file_name)} · ${book.chapter_count} chapter${book.chapter_count === 1 ? '' : 's'}</span>
        </button>
      `).join('')}
    </div>
  `;
}

function readerContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  if (state.error) {
    return `<div class="error-state"><strong>Could not open book</strong><span>${escapeHtml(state.error)}</span></div>`;
  }
  if (!book || !chapter) {
    return '<div class="empty-state"><strong>No book loaded</strong><span>Import an EPUB or select a .zmd book from the library.</span></div>';
  }
  const index = book.chapters.findIndex(item => item.path === chapter.path);
  const cached = renderedChapterCache.get(chapterCacheKey(book, chapter));
  return `
    <article class="reader-card" aria-label="Reader chapter. Swipe left or right to change chapters.">
      <style>${styleTagContent(scopeReaderCss(book.style_css) + "\n" + scopeReaderCss(state.themeCss ?? ''))}</style>
      <div class="book-content" data-render-chapter="${escapeHtml(chapter.path)}">${cached ?? loadingChapterMarkup(chapter)}</div>
      <div class="reader-nav" aria-label="Reader chapter navigation">
        <button id="previous-chapter" type="button" ${index <= 0 ? 'disabled' : ''}>Previous</button>
        <span class="reader-position" aria-live="polite">
          <span>Chapter ${index + 1} of ${book.chapters.length}</span>
          <span class="swipe-hint">Swipe left or right to change chapters</span>
        </span>
        <button id="next-chapter" type="button" ${index >= book.chapters.length - 1 ? 'disabled' : ''}>Next</button>
      </div>
    </article>
  `;
}

async function bootstrap(): Promise<void> {
  await Promise.all([loadAppPaths(), refreshLibrary(false), refreshThemes(false)]);
  renderShell();
}

async function loadAppPaths(): Promise<void> {
  try {
    state.appPaths = await invoke<AppPaths>('app_paths');
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
  }
}

async function refreshLibrary(shouldRender = true): Promise<void> {
  state.libraryLoading = true;
  if (shouldRender) renderShell();
  try {
    state.library = await invoke<LibraryBook[]>('list_books');
    state.libraryError = undefined;
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
  } finally {
    state.libraryLoading = false;
    if (shouldRender) renderShell();
  }
}

async function refreshThemes(shouldRender = true): Promise<void> {
  state.themeLoading = true;
  if (shouldRender) renderShell();
  try {
    state.themes = await invoke<ThemeEntry[]>('list_themes');
    state.themeError = undefined;
    if (state.themePath && !state.themes.some(theme => theme.path === state.themePath)) {
      resetThemeCss();
      return;
    }
  } catch (error) {
    state.themeError = error instanceof Error ? error.message : String(error);
  } finally {
    state.themeLoading = false;
    if (shouldRender) renderShell();
  }
}

async function importEpub(): Promise<void> {
  state.importing = true;
  state.error = undefined;
  renderShell();
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'EPUB book', extensions: ['epub'] }],
    });
    if (typeof selected !== 'string') return;
    const imported = await invoke<ImportPayload>('import_epub', { path: selected });
    openBookPayload(imported.payload, imported.book.path);
    await refreshLibrary(false);
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.importing = false;
    renderShell();
  }
}

async function openLibraryBook(path: string): Promise<void> {
  state.loading = true;
  state.error = undefined;
  state.selectedBookPath = path;
  renderShell();
  try {
    const book = await invoke<BookPayload>('load_book_file', { path });
    openBookPayload(book, path);
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    renderShell();
  }
}

function openBookPayload(book: BookPayload, path: string): void {
  renderedChapterCache.clear();
  state.book = book;
  state.selectedBookPath = path;
  state.selectedPath = book.chapters[0]?.path;
  state.pendingFragment = undefined;
}

async function selectTheme(path: string): Promise<void> {
  state.themeError = undefined;
  if (!path) {
    resetThemeCss();
    return;
  }
  state.themeLoading = true;
  renderShell();
  try {
    const theme = await invoke<ThemePayload>('load_theme_file', { path });
    state.themeCss = theme.css;
    state.themeName = theme.name;
    state.themePath = path;
  } catch (error) {
    state.themeError = error instanceof Error ? error.message : String(error);
  } finally {
    state.themeLoading = false;
    renderShell();
  }
}

function resetThemeCss(): void {
  state.themeCss = undefined;
  state.themeName = undefined;
  state.themePath = undefined;
  state.themeError = undefined;
  renderShell();
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

function currentChapter(): BookChapter | undefined {
  return state.book?.chapters.find(chapter => chapter.path === state.selectedPath) ?? state.book?.chapters[0];
}

function moveChapter(offset: ChapterOffset): void {
  const nextPath = targetChapterPath(state.book?.chapters ?? [], state.selectedPath, offset);
  if (!nextPath) return;
  state.selectedPath = nextPath;
  state.pendingFragment = undefined;
  renderShell();
  scrollReaderPaneToTop();
}

function bindReaderGestures(): void {
  const card = document.querySelector<HTMLElement>('.reader-card');
  if (!card || !state.book || !currentChapter()) return;

  card.addEventListener('pointerdown', event => {
    if (!event.isPrimary || event.button !== 0 || event.pointerType === 'mouse') return;
    swipeStart = { x: event.clientX, y: event.clientY, timeMs: event.timeStamp };
  });
  card.addEventListener('pointercancel', () => {
    swipeStart = undefined;
  });
  card.addEventListener('pointerup', event => {
    if (!event.isPrimary || !swipeStart) return;
    const decision = interpretHorizontalSwipe(swipeStart, { x: event.clientX, y: event.clientY, timeMs: event.timeStamp });
    swipeStart = undefined;
    if (!decision) return;
    event.preventDefault();
    suppressNextReaderClickUntil = Date.now() + 350;
    moveChapter(decision);
  });
  card.addEventListener('click', event => {
    if (Date.now() <= suppressNextReaderClickUntil) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, true);
  card.addEventListener('wheel', event => {
    if (Date.now() - lastWheelNavigationAt < 900) return;
    if (Math.abs(event.deltaX) <= Math.abs(event.deltaY) * 2) return;
    accumulatedHorizontalWheelDelta += event.deltaX;
    if (wheelResetTimer) window.clearTimeout(wheelResetTimer);
    wheelResetTimer = window.setTimeout(() => {
      accumulatedHorizontalWheelDelta = 0;
      wheelResetTimer = undefined;
    }, 180);

    const decision = interpretHorizontalWheel(accumulatedHorizontalWheelDelta, event.deltaY);
    if (!decision) return;
    event.preventDefault();
    accumulatedHorizontalWheelDelta = 0;
    lastWheelNavigationAt = Date.now();
    if (wheelResetTimer) {
      window.clearTimeout(wheelResetTimer);
      wheelResetTimer = undefined;
    }
    moveChapter(decision);
  }, { passive: false });
}

function scrollReaderPaneToTop(): void {
  requestAnimationFrame(() => {
    document.querySelector<HTMLElement>('.reader-pane')?.scrollTo({ top: 0, behavior: 'smooth' });
  });
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
void bootstrap();
