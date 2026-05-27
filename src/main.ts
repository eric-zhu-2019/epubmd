import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { extractChapterSections, type ChapterSection } from './chapterSections';
import { scopeReaderCss, styleTagContent } from './cssScope';
import { structureFlatTocMarkdown } from './tocStructure';
import { interpretHorizontalSwipe, interpretHorizontalWheel, targetChapterPath, type ChapterOffset, type GesturePoint } from './readerNavigation';
import './styles.css';

type LibraryBook = {
  path: string;
  file_name: string;
  title: string;
  chapter_count: number;
  progress_chapter_path?: string | null;
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

type ReadingProgress = {
  chapter_path: string;
  updated_ms: number;
};

type ColorMode = 'daylight' | 'dark';

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
  deletingBookPath?: string;
  libraryLoading: boolean;
  themeLoading: boolean;
  themeCss?: string;
  themeName?: string;
  themePath?: string;
  pendingFragment?: string;
  expandedChapterPaths: Set<string>;
  chapterListScrollTop: number;
  renderToken: number;
  colorMode: ColorMode;
  sidebarWidth: number;
  readerWidthCh: number;
  readerFontSizePx: number;
  settingsOpen: boolean;
};

const colorModeStorageKey = 'goosereader:color-mode';
const sidebarWidthStorageKey = 'goosereader:sidebar-width';
const readerWidthStorageKey = 'goosereader:reader-width-ch';
const readerFontSizeStorageKey = 'goosereader:reader-font-size-px';
const defaultSidebarWidth = 320;
const minSidebarWidth = 260;
const maxSidebarWidth = 560;
const defaultReaderWidthCh = 78;
const minReaderWidthCh = 52;
const maxReaderWidthCh = 110;
const defaultReaderFontSizePx = 17;
const minReaderFontSizePx = 14;
const maxReaderFontSizePx = 24;

const state: ReaderState = {
  library: [],
  themes: [],
  loading: false,
  importing: false,
  libraryLoading: true,
  themeLoading: true,
  expandedChapterPaths: new Set(),
  chapterListScrollTop: 0,
  renderToken: 0,
  colorMode: readStoredColorMode(),
  sidebarWidth: readStoredSidebarWidth(),
  readerWidthCh: readStoredNumber(readerWidthStorageKey, defaultReaderWidthCh, minReaderWidthCh, maxReaderWidthCh),
  readerFontSizePx: readStoredNumber(readerFontSizeStorageKey, defaultReaderFontSizePx, minReaderFontSizePx, maxReaderFontSizePx),
  settingsOpen: false,
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
const appLogoUrl = new URL('./assets/goosereader-logo.png', import.meta.url).href;

applyColorMode(state.colorMode);

marked.use({
  gfm: true,
  breaks: false,
});

function renderShell(options: { preserveChapterScroll?: boolean } = {}): void {
  if (options.preserveChapterScroll !== false) captureChapterListScroll();
  const book = state.book;
  const current = currentChapter();
  app.innerHTML = `
    <main class="shell" style="--sidebar-width: ${state.sidebarWidth}px">
      <aside class="sidebar">
        <div class="brand">
          <div class="brand-lockup">
            <img class="brand-logo" src="${appLogoUrl}" alt="" aria-hidden="true" />
            <div class="brand-copy">
              <div class="brand-title-row">
                <h1>goosereader</h1>
                <button id="settings-toggle" class="header-icon-button settings-toggle" type="button" aria-expanded="${state.settingsOpen}" aria-controls="settings-menu" aria-label="${state.settingsOpen ? 'Hide' : 'Show'} settings" title="Settings">
                  <span class="toggle-icon" aria-hidden="true">⚙</span>
                </button>
                <button id="color-mode-toggle" class="header-icon-button color-mode-toggle" type="button" aria-pressed="${state.colorMode === 'dark'}" aria-label="Switch to ${state.colorMode === 'dark' ? 'daylight' : 'dark'} mode" title="${state.colorMode === 'dark' ? 'Dark mode' : 'Daylight mode'}">
                  <span class="toggle-icon" aria-hidden="true">${state.colorMode === 'dark' ? '☾' : '☀'}</span>
                </button>
              </div>
              <span class="brand-subtitle">Markdown goose reader</span>
            </div>
          </div>
          <div class="brand-actions">
            <button id="import-epub" class="import-button" type="button" ${state.importing || state.loading ? 'disabled' : ''}>${state.importing ? 'Importing…' : 'Import EPUB'}</button>
          </div>
        </div>

        ${settingsMenuContent()}

        <section class="sidebar-section library-panel" aria-label="Library">
          <div class="section-heading">
            <span>Library</span>
            <button id="refresh-library" class="link-button" type="button">Refresh</button>
          </div>
          <p class="folder-hint">${escapeHtml(state.appPaths?.books_dir ?? '~/.config/goosereader/books')}</p>
          ${state.libraryError ? `<p class="inline-error">${escapeHtml(state.libraryError)}</p>` : ''}
          ${libraryContent()}
        </section>

        <section class="sidebar-section chapter-panel" aria-label="Chapters">
          <div class="section-heading"><span>Chapters</span></div>
          <p class="book-title">${escapeHtml(book?.title ?? 'Select a book to start reading.')}</p>
          <nav class="chapter-list" aria-label="Chapters">
            ${chapterListContent(book)}
          </nav>
        </section>
        <div
          id="sidebar-resize-handle"
          class="sidebar-resize-handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          aria-valuemin="${minSidebarWidth}"
          aria-valuemax="${maxSidebarWidth}"
          aria-valuenow="${state.sidebarWidth}"
          tabindex="0"
          title="Drag to resize sidebar"
        ></div>
      </aside>
      <section class="reader-pane">
        ${readerToolbarContent(book, current)}
        <div class="reader-stage">
          ${readerContent(book, current)}
        </div>
      </section>
    </main>
  `;
  document.querySelector<HTMLButtonElement>('#settings-toggle')?.addEventListener('click', () => {
    state.settingsOpen = !state.settingsOpen;
    renderShell();
  });
  document.querySelector<HTMLButtonElement>('#color-mode-toggle')?.addEventListener('click', toggleColorMode);
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
  document.querySelectorAll<HTMLButtonElement>('[data-delete-book]').forEach(button => {
    button.addEventListener('click', () => {
      const path = button.dataset.deleteBook;
      const title = button.dataset.deleteTitle;
      if (path && title) void deleteLibraryBook(path, title);
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-toggle-chapter]').forEach(button => {
    button.addEventListener('click', () => {
      const path = button.dataset.toggleChapter;
      if (!path) return;
      if (state.expandedChapterPaths.has(path)) state.expandedChapterPaths.delete(path);
      else state.expandedChapterPaths.add(path);
      renderShell();
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-chapter]').forEach(button => {
    button.addEventListener('click', () => {
      if (button.dataset.chapter) selectChapter(button.dataset.chapter);
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-section-chapter][data-section-id]').forEach(button => {
    button.addEventListener('click', () => {
      const chapterPath = button.dataset.sectionChapter;
      const sectionId = button.dataset.sectionId;
      if (!chapterPath || !sectionId) return;
      selectChapter(chapterPath, { fragment: sectionId });
    });
  });
  document.querySelector<HTMLButtonElement>('#previous-chapter')?.addEventListener('click', () => moveChapter(-1));
  document.querySelector<HTMLButtonElement>('#next-chapter')?.addEventListener('click', () => moveChapter(1));
  bindSidebarResize();
  bindReaderSettings();
  bindReaderGestures();
  bindReaderLinks();
  restoreChapterListScroll();
  void renderSelectedChapter(book, current);
}


function toggleColorMode(): void {
  state.colorMode = state.colorMode === 'dark' ? 'daylight' : 'dark';
  applyColorMode(state.colorMode);
  try {
    window.localStorage.setItem(colorModeStorageKey, state.colorMode);
  } catch {
    // Ignore storage failures; the in-memory toggle should still work.
  }
  renderShell();
}

function applyColorMode(mode: ColorMode): void {
  document.documentElement.dataset.colorMode = mode;
}

function readStoredColorMode(): ColorMode {
  try {
    return window.localStorage.getItem(colorModeStorageKey) === 'dark' ? 'dark' : 'daylight';
  } catch {
    return 'daylight';
  }
}

function readStoredSidebarWidth(): number {
  try {
    const stored = Number(window.localStorage.getItem(sidebarWidthStorageKey));
    return clampSidebarWidth(Number.isFinite(stored) ? stored : defaultSidebarWidth);
  } catch {
    return defaultSidebarWidth;
  }
}

function readStoredNumber(key: string, fallback: number, min: number, max: number): number {
  try {
    const stored = Number(window.localStorage.getItem(key));
    return clampNumber(Number.isFinite(stored) ? stored : fallback, min, max);
  } catch {
    return fallback;
  }
}

function clampNumber(value: number, min: number, max: number): number {
  return Math.round(Math.min(max, Math.max(min, value)));
}

function persistSidebarWidth(): void {
  try {
    window.localStorage.setItem(sidebarWidthStorageKey, String(Math.round(state.sidebarWidth)));
  } catch {
    // Ignore storage failures; the current layout should still update in memory.
  }
}

function persistReaderSettings(): void {
  try {
    window.localStorage.setItem(readerWidthStorageKey, String(state.readerWidthCh));
    window.localStorage.setItem(readerFontSizeStorageKey, String(state.readerFontSizePx));
  } catch {
    // Ignore storage failures; the current layout should still update in memory.
  }
}

function settingsMenuContent(): string {
  if (!state.settingsOpen) return '';
  return `
    <div id="settings-menu" class="settings-menu" role="region" aria-label="Settings">
      <section class="sidebar-section theme-panel" aria-label="Theme controls">
        <div class="section-heading">
          <span>Theme</span>
          <button id="refresh-themes" class="link-button" type="button">Refresh</button>
        </div>
        <p class="folder-hint">${escapeHtml(state.appPaths?.themes_dir ?? '~/.config/goosereader/themes')}</p>
        <select id="theme-select" class="theme-select" ${state.themeLoading ? 'disabled' : ''}>
          <option value="">Default archive style</option>
          ${state.themes.map(theme => `
            <option value="${escapeHtml(theme.path)}" ${theme.path === state.themePath ? 'selected' : ''}>${escapeHtml(theme.name)}</option>
          `).join('')}
        </select>
        ${state.themeName ? `<span class="theme-name">Using ${escapeHtml(state.themeName)}</span>` : ''}
        ${state.themeError ? `<span class="theme-error">${escapeHtml(state.themeError)}</span>` : ''}
      </section>

      <section class="sidebar-section reader-settings-panel" aria-label="Reader settings">
        <div class="section-heading"><span>Reader</span></div>
        <label class="reader-setting" for="reader-width">
          <span class="reader-setting-label">
            <span>Page width</span>
            <output id="reader-width-value" for="reader-width">${state.readerWidthCh}ch</output>
          </span>
          <input id="reader-width" type="range" min="${minReaderWidthCh}" max="${maxReaderWidthCh}" step="2" value="${state.readerWidthCh}" />
        </label>
        <label class="reader-setting" for="reader-font-size">
          <span class="reader-setting-label">
            <span>Font size</span>
            <output id="reader-font-size-value" for="reader-font-size">${state.readerFontSizePx}px</output>
          </span>
          <input id="reader-font-size" type="range" min="${minReaderFontSizePx}" max="${maxReaderFontSizePx}" step="1" value="${state.readerFontSizePx}" />
        </label>
      </section>
    </div>
  `;
}

function readerToolbarContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  return `
    <header class="reader-toolbar" aria-label="Reader toolbar">
      <span class="reader-toolbar-title">${escapeHtml(book?.title ?? 'goosereader')}</span>
      <span class="reader-toolbar-subtitle">${escapeHtml(chapter?.title ?? 'No book loaded')}</span>
    </header>
  `;
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
        <div class="book-item ${book.path === state.selectedBookPath ? 'active' : ''}">
          <button class="book-button ${book.path === state.selectedBookPath ? 'active' : ''}" type="button" data-book="${escapeHtml(book.path)}" ${state.loading || Boolean(state.deletingBookPath) ? 'disabled' : ''}>
            <span class="book-button-title">${escapeHtml(state.loading && book.path === state.selectedBookPath ? 'Opening…' : book.title)}</span>
            <span class="book-button-meta">${escapeHtml(bookMeta(book))}</span>
          </button>
          <button class="delete-book-button" type="button" data-delete-book="${escapeHtml(book.path)}" data-delete-title="${escapeHtml(book.title)}" ${state.deletingBookPath ? 'disabled' : ''} aria-label="Delete ${escapeHtml(book.title)}">
            ${state.deletingBookPath === book.path ? 'Deleting…' : 'Delete'}
          </button>
        </div>
      `).join('')}
    </div>
  `;
}

function bookMeta(book: LibraryBook): string {
  const chapterLabel = `${book.chapter_count} chapter${book.chapter_count === 1 ? '' : 's'}`;
  const progressLabel = book.progress_chapter_path ? ' · progress saved' : '';
  return `${book.file_name} · ${chapterLabel}${progressLabel}`;
}

function chapterListContent(book: BookPayload | undefined): string {
  if (!book) return '';
  return book.chapters.map(chapter => {
    const isActive = chapter.path === state.selectedPath;
    const sections = extractChapterSections(chapter.markdown);
    const isExpanded = isActive || state.expandedChapterPaths.has(chapter.path);
    const sectionList = isExpanded && sections.length > 0 ? `
      <div class="chapter-section-list" role="group" aria-label="Sections in ${escapeHtml(chapter.title)}">
        ${sections.map(section => sectionButton(chapter, section)).join('')}
      </div>
    ` : '';
    return `
      <div class="chapter-item ${isActive ? 'active' : ''}">
        <div class="chapter-row">
          <button class="chapter-button ${isActive ? 'active' : ''}" type="button" data-chapter="${escapeHtml(chapter.path)}">
            ${escapeHtml(chapter.title)}
          </button>
          ${sections.length > 0 ? `
            <button class="chapter-toggle" type="button" data-toggle-chapter="${escapeHtml(chapter.path)}" aria-label="${isExpanded ? 'Collapse' : 'Expand'} sections for ${escapeHtml(chapter.title)}" aria-expanded="${isExpanded}">
              ${isExpanded ? '▾' : '▸'}
            </button>
          ` : ''}
        </div>
        ${sectionList}
      </div>
    `;
  }).join('');
}

function sectionButton(chapter: BookChapter, section: ChapterSection): string {
  const indent = Math.max(0, Math.min(section.level - 1, 4));
  return `
    <button class="section-button depth-${indent}" type="button" data-section-chapter="${escapeHtml(chapter.path)}" data-section-id="${escapeHtml(section.id)}">
      ${escapeHtml(section.title)}
    </button>
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
    <article class="reader-card" style="${readerPreferenceStyle()}" aria-label="Reader chapter. Swipe left or right to change chapters.">
      <style>${styleTagContent(scopeReaderCss(book.style_css) + "\n" + scopeReaderCss(state.themeCss ?? '') + "\n" + readerPreferenceCss() + "\n" + readerColorModeCss())}</style>
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

function readerPreferenceStyle(): string {
  return `--reader-content-width: ${state.readerWidthCh}ch; --reader-font-size: ${state.readerFontSizePx}px`;
}

function readerPreferenceCss(): string {
  return `
    .reader-card .book-content {
      max-width: var(--reader-content-width) !important;
      font-size: var(--reader-font-size) !important;
    }

    .reader-card .book-content p,
    .reader-card .book-content li,
    .reader-card .book-content blockquote,
    .reader-card .book-content td,
    .reader-card .book-content th {
      font-size: inherit !important;
    }
  `;
}

function readerColorModeCss(): string {
  if (state.colorMode !== 'dark') return '';
  return `
    html[data-color-mode="dark"] .reader-card,
    html[data-color-mode="dark"] .reader-card .book-content {
      background: var(--reader-surface) !important;
      color: var(--reader-text) !important;
    }

    html[data-color-mode="dark"] .reader-card .book-content h1,
    html[data-color-mode="dark"] .reader-card .book-content h2,
    html[data-color-mode="dark"] .reader-card .book-content h3,
    html[data-color-mode="dark"] .reader-card .book-content h4,
    html[data-color-mode="dark"] .reader-card .book-content h5,
    html[data-color-mode="dark"] .reader-card .book-content h6,
    html[data-color-mode="dark"] .reader-card .book-content p,
    html[data-color-mode="dark"] .reader-card .book-content li,
    html[data-color-mode="dark"] .reader-card .book-content blockquote,
    html[data-color-mode="dark"] .reader-card .book-content table,
    html[data-color-mode="dark"] .reader-card .book-content td,
    html[data-color-mode="dark"] .reader-card .book-content th {
      color: var(--reader-text) !important;
    }

    html[data-color-mode="dark"] .reader-card .book-content a {
      color: var(--link) !important;
    }

    html[data-color-mode="dark"] .reader-card .book-content pre,
    html[data-color-mode="dark"] .reader-card .book-content code {
      background: #0b1220 !important;
      color: #eef3fb !important;
    }

    html[data-color-mode="dark"] .reader-card .book-content blockquote {
      border-left-color: var(--border-strong) !important;
    }
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
    await saveCurrentReadingProgress();
    await refreshLibrary(false);
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.importing = false;
    renderShell({ preserveChapterScroll: false });
  }
}

async function openLibraryBook(path: string): Promise<void> {
  state.loading = true;
  state.error = undefined;
  state.selectedBookPath = path;
  renderShell();
  try {
    const book = await invoke<BookPayload>('load_book_file', { path });
    const progress = await loadReadingProgress(path);
    openBookPayload(book, path, progress?.chapter_path);
    await saveCurrentReadingProgress();
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    renderShell({ preserveChapterScroll: false });
  }
}

async function deleteLibraryBook(path: string, title: string): Promise<void> {
  if (!window.confirm(`Delete "${title}" from your goosereader library? This removes the local .zmd file.`)) return;
  state.deletingBookPath = path;
  state.libraryError = undefined;
  renderShell();
  try {
    await invoke<void>('delete_book', { path });
    if (state.selectedBookPath === path) {
      renderedChapterCache.clear();
      state.book = undefined;
      state.selectedBookPath = undefined;
      state.selectedPath = undefined;
      state.pendingFragment = undefined;
      state.expandedChapterPaths = new Set();
      state.chapterListScrollTop = 0;
    }
    await refreshLibrary(false);
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
  } finally {
    state.deletingBookPath = undefined;
    renderShell();
  }
}

async function loadReadingProgress(path: string): Promise<ReadingProgress | undefined> {
  const progress = await invoke<ReadingProgress | null>('load_reading_progress', { path });
  return progress ?? undefined;
}

async function saveCurrentReadingProgress(): Promise<void> {
  if (!state.selectedBookPath || !state.selectedPath) return;
  try {
    await invoke<void>('save_reading_progress', {
      path: state.selectedBookPath,
      chapterPath: state.selectedPath,
    });
    const book = state.library.find(book => book.path === state.selectedBookPath);
    if (book) book.progress_chapter_path = state.selectedPath;
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
    renderShell();
  }
}

function openBookPayload(book: BookPayload, path: string, progressChapterPath?: string): void {
  renderedChapterCache.clear();
  state.book = book;
  state.selectedBookPath = path;
  const progressChapter = book.chapters.find(chapter => chapter.path === progressChapterPath);
  const selectedPath = progressChapter?.path ?? book.chapters[0]?.path;
  state.selectedPath = selectedPath;
  state.pendingFragment = undefined;
  state.expandedChapterPaths = new Set([selectedPath].filter((path): path is string => Boolean(path)));
  state.chapterListScrollTop = 0;
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
    applySectionIds(target, chapter);
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
  applySectionIds(target, chapter);
  const html = target.innerHTML;
  renderedChapterCache.set(key, html);
  bindReaderLinks(target);
  scrollPendingFragment();
}

function renderMarkdownFragment(book: BookPayload, chapter: BookChapter, markdown: string): string {
  const rendered = marked.parse(structureFlatTocMarkdown(markdown), { async: false }) as string;
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
  selectChapter(nextPath);
  scrollReaderPaneToTop();
}

function selectChapter(chapterPath: string, options: { fragment?: string } = {}): void {
  state.selectedPath = chapterPath;
  state.error = undefined;
  state.pendingFragment = options.fragment;
  state.expandedChapterPaths.add(chapterPath);
  renderShell();
  void saveCurrentReadingProgress();
}

function captureChapterListScroll(): void {
  const chapterList = document.querySelector<HTMLElement>('.chapter-list');
  if (chapterList) state.chapterListScrollTop = chapterList.scrollTop;
}

function restoreChapterListScroll(): void {
  const chapterList = document.querySelector<HTMLElement>('.chapter-list');
  if (!chapterList) return;
  chapterList.scrollTop = state.chapterListScrollTop;
}

function bindSidebarResize(): void {
  const handle = document.querySelector<HTMLElement>('#sidebar-resize-handle');
  const shell = document.querySelector<HTMLElement>('.shell');
  if (!handle || !shell) return;

  const applyWidth = (width: number): void => {
    const clamped = clampSidebarWidth(width);
    state.sidebarWidth = clamped;
    shell.style.setProperty('--sidebar-width', `${clamped}px`);
    handle.setAttribute('aria-valuenow', String(clamped));
  };

  handle.addEventListener('pointerdown', event => {
    if (!event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    document.body.classList.add('is-resizing-sidebar');

    const resizeTo = (clientX: number): void => {
      const shellLeft = shell.getBoundingClientRect().left;
      applyWidth(clientX - shellLeft);
    };
    const onPointerMove = (moveEvent: PointerEvent): void => {
      if (!moveEvent.isPrimary) return;
      resizeTo(moveEvent.clientX);
    };
    const finishResize = (): void => {
      document.removeEventListener('pointermove', onPointerMove);
      document.removeEventListener('pointerup', finishResize);
      document.removeEventListener('pointercancel', finishResize);
      document.body.classList.remove('is-resizing-sidebar');
      persistSidebarWidth();
    };

    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', finishResize);
    document.addEventListener('pointercancel', finishResize);
  });

  handle.addEventListener('keydown', event => {
    const step = event.shiftKey ? 40 : 16;
    let nextWidth: number | undefined;
    if (event.key === 'ArrowLeft') nextWidth = state.sidebarWidth - step;
    if (event.key === 'ArrowRight') nextWidth = state.sidebarWidth + step;
    if (event.key === 'Home') nextWidth = minSidebarWidth;
    if (event.key === 'End') nextWidth = maxSidebarWidth;
    if (nextWidth === undefined) return;
    event.preventDefault();
    applyWidth(nextWidth);
    persistSidebarWidth();
  });
}

function clampSidebarWidth(width: number): number {
  const viewportMax = Math.max(minSidebarWidth, Math.min(maxSidebarWidth, Math.floor(window.innerWidth * 0.6)));
  return Math.round(Math.min(viewportMax, Math.max(minSidebarWidth, width)));
}

function bindReaderSettings(): void {
  const widthInput = document.querySelector<HTMLInputElement>('#reader-width');
  const widthValue = document.querySelector<HTMLOutputElement>('#reader-width-value');
  const fontSizeInput = document.querySelector<HTMLInputElement>('#reader-font-size');
  const fontSizeValue = document.querySelector<HTMLOutputElement>('#reader-font-size-value');

  widthInput?.addEventListener('input', () => {
    state.readerWidthCh = clampNumber(Number(widthInput.value), minReaderWidthCh, maxReaderWidthCh);
    widthInput.value = String(state.readerWidthCh);
    if (widthValue) widthValue.value = `${state.readerWidthCh}ch`;
    applyReaderSettings();
  });
  widthInput?.addEventListener('change', persistReaderSettings);

  fontSizeInput?.addEventListener('input', () => {
    state.readerFontSizePx = clampNumber(Number(fontSizeInput.value), minReaderFontSizePx, maxReaderFontSizePx);
    fontSizeInput.value = String(state.readerFontSizePx);
    if (fontSizeValue) fontSizeValue.value = `${state.readerFontSizePx}px`;
    applyReaderSettings();
  });
  fontSizeInput?.addEventListener('change', persistReaderSettings);
}

function applyReaderSettings(): void {
  document.querySelector<HTMLElement>('.reader-card')?.setAttribute('style', readerPreferenceStyle());
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
  selectChapter(target.path, { fragment });
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

function applySectionIds(scope: ParentNode, chapter: BookChapter): void {
  const sections = extractChapterSections(chapter.markdown);
  if (sections.length === 0) return;
  const headings = Array.from(scope.querySelectorAll<HTMLHeadingElement>('h1, h2, h3, h4, h5, h6'));
  headings.forEach((heading, index) => {
    const section = sections[index];
    if (section && !heading.id) heading.id = section.id;
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
