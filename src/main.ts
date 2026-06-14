import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow, type Theme } from '@tauri-apps/api/window';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import { extractChapterSections, type ChapterSection } from './chapterSections';
import { scopeReaderCss, styleTagContent } from './cssScope';
import { structureFlatTocMarkdown } from './tocStructure';
import { findTextMatches, normalizeQuery, type TextSearchMatch } from './textSearch';
import { interpretHorizontalSwipe, interpretHorizontalWheel, targetChapterPath, type ChapterOffset, type GesturePoint } from './readerNavigation';
import './styles.css';

type LibraryBook = {
  path: string;
  file_name: string;
  title: string;
  chapter_count: number;
  progress_chapter_path?: string | null;
  reading_progress_percent?: number | null;
  completed: boolean;
  modified_ms: number;
  cover_image?: string | null;
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

type SystemAppearancePayload = {
  color_mode: ResolvedColorMode;
  source: string;
};

type ReadingProgress = {
  chapter_path: string;
  updated_ms: number;
  completed: boolean;
  page_index?: number | null;
  scroll_top?: number | null;
};

type ColorMode = 'system' | 'daylight' | 'dark';
type ResolvedColorMode = 'daylight' | 'dark';
type ReaderMode = 'scroll' | 'paged';
type ReaderFontFamily = 'literata' | 'new-york' | 'iowan' | 'charter' | 'georgia' | 'palatino' | 'baskerville';

type ReaderFontOption = {
  value: ReaderFontFamily;
  label: string;
  detail: string;
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
  deletingBookPath?: string;
  libraryLoading: boolean;
  themeLoading: boolean;
  themeCss?: string;
  themeName?: string;
  themePath?: string;
  pendingFragment?: string;
  expandedChapterPaths: Set<string>;
  chapterListScrollTop: number;
  readerPaneScrollTop: number;
  renderToken: number;
  colorMode: ColorMode;
  readerMode: ReaderMode;
  sidebarWidth: number;
  librarySidebarHidden: boolean;
  outlineSidebarHidden: boolean;
  readerWidthCh: number;
  readerFontSizePx: number;
  readerFontFamily: ReaderFontFamily;
  readerFontMenuOpen: boolean;
  settingsOpen: boolean;
  librarySearch: string;
  readerSearchQuery: string;
  searchActiveIndex: number;
  pendingSearchScroll: boolean;
  pageIndex: number;
  pageCount: number;
  pendingPageTarget?: 'start' | 'end';
  pendingScrollRestore: boolean;
  homeDeleteMode: boolean;
  libraryContextMenu?: {
    path: string;
    title: string;
    x: number;
    y: number;
  };
};

const colorModeStorageKey = 'goosereader:color-mode';
const readerModeStorageKey = 'goosereader:reader-mode';
const sidebarWidthStorageKey = 'goosereader:sidebar-width';
const librarySidebarHiddenStorageKey = 'goosereader:library-sidebar-hidden';
const outlineSidebarHiddenStorageKey = 'goosereader:outline-sidebar-hidden';
const readerWidthStorageKey = 'goosereader:reader-width-ch';
const readerFontSizeStorageKey = 'goosereader:reader-font-size-px';
const readerFontFamilyStorageKey = 'goosereader:reader-font-family';
const readerFontOptions: ReaderFontOption[] = [
  { value: 'literata', label: 'Literata', detail: 'Book serif' },
  { value: 'new-york', label: 'New York', detail: 'Apple serif' },
  { value: 'iowan', label: 'Iowan', detail: 'Classic serif' },
  { value: 'charter', label: 'Charter', detail: 'Screen serif' },
  { value: 'georgia', label: 'Georgia', detail: 'Web serif' },
  { value: 'palatino', label: 'Palatino', detail: 'Wide serif' },
  { value: 'baskerville', label: 'Baskerville', detail: 'Literary serif' },
];
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
  readerPaneScrollTop: 0,
  renderToken: 0,
  colorMode: readStoredColorMode(),
  readerMode: readStoredReaderMode(),
  sidebarWidth: readStoredSidebarWidth(),
  librarySidebarHidden: readStoredBoolean(librarySidebarHiddenStorageKey, false),
  outlineSidebarHidden: readStoredBoolean(outlineSidebarHiddenStorageKey, false),
  readerWidthCh: readStoredNumber(readerWidthStorageKey, defaultReaderWidthCh, minReaderWidthCh, maxReaderWidthCh),
  readerFontSizePx: readStoredNumber(readerFontSizeStorageKey, defaultReaderFontSizePx, minReaderFontSizePx, maxReaderFontSizePx),
  readerFontFamily: readStoredReaderFontFamily(),
  readerFontMenuOpen: false,
  settingsOpen: false,
  librarySearch: '',
  readerSearchQuery: '',
  searchActiveIndex: -1,
  pendingSearchScroll: false,
  pageIndex: 0,
  pageCount: 1,
  pendingScrollRestore: false,
  homeDeleteMode: false,
};
const appElement = document.querySelector<HTMLDivElement>('#app');
if (!appElement) throw new Error('missing #app');
const app: HTMLDivElement = appElement;
const renderedChapterCache = new Map<string, string>();
const assetIndexCache = new WeakMap<BookPayload, Map<string, string>>();
const systemColorScheme = window.matchMedia('(prefers-color-scheme: dark)');
let nativeResolvedColorMode: ResolvedColorMode | undefined;
let nativeResolvedColorModeSource = '';
let swipeStart: GesturePoint | undefined;
let suppressNextReaderClickUntil = 0;
let accumulatedHorizontalWheelDelta = 0;
let wheelResetTimer: number | undefined;
let lastWheelNavigationAt = 0;
let homeLongPressTimer: number | undefined;
let suppressHomeBookOpenUntil = 0;
let lastHomeBookPointerType = 'mouse';
let lastHomeBookPointerAt = 0;
let searchCacheBook: BookPayload | undefined;
let searchCacheQuery = '';
let searchCacheMatches: TextSearchMatch[] = [];
let pagedLayoutTimer: number | undefined;
let progressSaveTimer: number | undefined;
const appLogoUrl = new URL('./assets/goosereader-logo.png', import.meta.url).href;

document.documentElement.classList.toggle('tauri-runtime', hasTauriRuntime());
applyColorMode(resolveColorMode(state.colorMode));

marked.use({
  gfm: true,
  breaks: false,
});

function renderShell(options: { preserveChapterScroll?: boolean; preserveReaderScroll?: boolean } = {}): void {
  if (options.preserveChapterScroll !== false) captureChapterListScroll();
  if (options.preserveReaderScroll !== false) captureReaderPaneScroll();
  const book = state.book;
  const current = currentChapter();
  const resolvedColorMode = resolveColorMode(state.colorMode);
  const shellClasses = [
    'shell',
    state.librarySidebarHidden ? 'library-hidden' : '',
    state.outlineSidebarHidden ? 'outline-hidden' : '',
  ].filter(Boolean).join(' ');
  app.innerHTML = `
    <div class="app-frame">
      ${appTitlebarContent(book, resolvedColorMode)}
      <main class="${shellClasses}" style="--library-column-width: ${state.librarySidebarHidden ? '0px' : 'var(--library-width)'}; --sidebar-width: ${state.outlineSidebarHidden ? 0 : state.sidebarWidth}px">
      ${state.librarySidebarHidden ? '' : `
      <aside class="library-sidebar">
        <div class="library-sidebar-toolbar">
          <button id="import-epub" class="import-button" type="button" ${state.importing || state.loading ? 'disabled' : ''}>
            <span aria-hidden="true">+</span>
            <span>${state.importing ? 'Importing…' : 'Import EPUB'}</span>
          </button>
          <button id="settings-toggle" class="header-icon-button settings-toggle" type="button" aria-expanded="${state.settingsOpen}" aria-controls="settings-menu" aria-label="${state.settingsOpen ? 'Hide' : 'Show'} settings" title="Settings">
            <span class="toggle-icon" aria-hidden="true">⚙</span>
          </button>
          <button class="header-icon-button" type="button" data-toggle-library-sidebar aria-pressed="true" aria-label="Hide library sidebar" title="Hide library sidebar">
            <span class="toggle-icon" aria-hidden="true">×</span>
          </button>
        </div>

        ${sidebarNavigationContent()}
        ${settingsMenuContent(resolvedColorMode)}
        ${librarySidebarBooksContent()}

        <footer class="library-sidebar-footer" aria-label="Library status">
          <span>${state.library.length} book${state.library.length === 1 ? '' : 's'}</span>
          <span>${state.library.filter(book => book.completed).length} done</span>
        </footer>
      </aside>
      `}
      ${state.outlineSidebarHidden ? '' : `
      <aside class="outline-sidebar">
        <section class="sidebar-section chapter-panel" aria-label="Chapters">
          <div class="section-heading outline-heading">
            <span>Outline</span>
            <button class="link-button icon-link-button" type="button" data-toggle-outline-sidebar aria-pressed="true" aria-label="Hide outline sidebar" title="Hide outline sidebar">×</button>
          </div>
          <p class="book-title">${escapeHtml(book?.title ?? 'Select a book to browse chapters.')}</p>
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
      `}
      <section class="reader-pane">
        ${readerToolbarContent(book, current)}
        <div class="reader-stage">
          ${readerContent(book, current)}
        </div>
        ${readerNavigationContent(book, current)}
      </section>
      ${libraryContextMenuContent()}
      </main>
    </div>
  `;
  bindTitlebarControls();
  document.querySelector<HTMLButtonElement>('#settings-toggle')?.addEventListener('click', () => {
    state.settingsOpen = !state.settingsOpen;
    renderShell();
  });
  document.querySelector<HTMLInputElement>('#library-search')?.addEventListener('input', event => {
    const input = event.currentTarget as HTMLInputElement;
    const cursor = input.selectionStart ?? input.value.length;
    state.librarySearch = input.value;
    renderShell();
    requestAnimationFrame(() => {
      const nextInput = document.querySelector<HTMLInputElement>('#library-search');
      nextInput?.focus();
      nextInput?.setSelectionRange(cursor, cursor);
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-toggle-library-sidebar]').forEach(button => {
    button.addEventListener('click', toggleLibrarySidebar);
  });
  document.querySelectorAll<HTMLButtonElement>('[data-toggle-outline-sidebar]').forEach(button => {
    button.addEventListener('click', toggleOutlineSidebar);
  });
  document.querySelector<HTMLButtonElement>('#home-button')?.addEventListener('click', showHome);
  document.querySelector<HTMLButtonElement>('#all-books-button')?.addEventListener('click', showHome);
  document.querySelector<HTMLButtonElement>('#done-delete-mode')?.addEventListener('click', () => {
    state.homeDeleteMode = false;
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
      if (Date.now() <= suppressHomeBookOpenUntil || state.homeDeleteMode || state.libraryContextMenu) return;
      if (button.dataset.book) void openLibraryBook(button.dataset.book);
    });
  });
  document.querySelectorAll<HTMLElement>('[data-home-book-card]').forEach(card => {
    card.addEventListener('contextmenu', event => {
      const path = card.dataset.menuBook;
      const title = card.dataset.menuTitle;
      if (!path || !title) return;
      event.preventDefault();
      if (lastHomeBookPointerType !== 'mouse' && Date.now() - lastHomeBookPointerAt < 1_500) {
        enterHomeDeleteMode();
        return;
      }
      state.homeDeleteMode = false;
      state.libraryContextMenu = {
        path,
        title,
        x: event.clientX,
        y: event.clientY,
      };
      renderShell();
    });
  });
  document.querySelectorAll<HTMLButtonElement>('[data-delete-book]').forEach(button => {
    button.addEventListener('click', () => {
      const path = button.dataset.deleteBook;
      const title = button.dataset.deleteTitle;
      if (path && title) void deleteLibraryBook(path, title);
    });
  });
  document.querySelector<HTMLButtonElement>('[data-context-delete-book]')?.addEventListener('click', event => {
    const button = event.currentTarget as HTMLButtonElement;
    const path = button.dataset.contextDeleteBook;
    const title = button.dataset.contextDeleteTitle;
    if (path && title) void deleteLibraryBook(path, title);
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
  document.querySelector<HTMLButtonElement>('#previous-page')?.addEventListener('click', () => moveReaderPage(-1));
  document.querySelector<HTMLButtonElement>('#next-page')?.addEventListener('click', () => moveReaderPage(1));
  bindSidebarResize();
  bindReaderSettings();
  bindHomeBookLongPress();
  bindLibraryContextMenuDismiss();
  bindReaderGestures();
  bindReaderLinks();
  bindReaderProgressTracking();
  restoreChapterListScroll();
  if (options.preserveReaderScroll !== false) restoreReaderPaneScroll();
  void renderSelectedChapter(book, current);
}

function bindTitlebarControls(): void {
  bindAppTitlebar();
  document.querySelector<HTMLInputElement>('#reader-search')?.addEventListener('input', event => {
    const input = event.currentTarget as HTMLInputElement;
    updateReaderSearch(input.value, input.selectionStart ?? input.value.length);
  });
  document.querySelector<HTMLInputElement>('#reader-search')?.addEventListener('keydown', event => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    moveSearchResult(event.shiftKey ? -1 : 1);
  });
  document.querySelector<HTMLButtonElement>('#search-previous')?.addEventListener('click', () => moveSearchResult(-1));
  document.querySelector<HTMLButtonElement>('#search-next')?.addEventListener('click', () => moveSearchResult(1));
  document.querySelector<HTMLButtonElement>('#search-clear')?.addEventListener('click', clearReaderSearch);
  document.querySelector<HTMLButtonElement>('#toggle-completed')?.addEventListener('click', () => void toggleSelectedBookCompleted());
  document.querySelectorAll<HTMLButtonElement>('[data-color-mode-choice]').forEach(button => {
    button.addEventListener('click', () => setColorMode(button.dataset.colorModeChoice as ColorMode));
  });
  document.querySelector<HTMLButtonElement>('#font-size-decrease')?.addEventListener('click', () => adjustReaderFontSize(-1));
  document.querySelector<HTMLButtonElement>('#font-size-increase')?.addEventListener('click', () => adjustReaderFontSize(1));
  document.querySelector<HTMLButtonElement>('#reader-font-menu-button')?.addEventListener('click', event => {
    event.stopPropagation();
    toggleReaderFontMenu();
  });
  document.querySelectorAll<HTMLButtonElement>('[data-reader-font-choice]').forEach(button => {
    button.addEventListener('click', () => setReaderFontFamily(button.dataset.readerFontChoice));
  });
  if (state.readerFontMenuOpen) {
    window.setTimeout(() => {
      document.addEventListener('click', closeReaderFontMenuFromDocument, { once: true });
    }, 0);
  }
}

function refreshAppTitlebar(): void {
  const titlebar = document.querySelector<HTMLElement>('.app-titlebar');
  if (!titlebar) {
    renderShell();
    return;
  }
  titlebar.outerHTML = appTitlebarContent(state.book, resolveColorMode(state.colorMode));
  bindTitlebarControls();
}

function appTitlebarContent(book: BookPayload | undefined, resolvedColorMode: ResolvedColorMode): string {
  const matches = currentSearchMatches();
  const hasQuery = normalizeQuery(state.readerSearchQuery).length > 0;
  const activeSearchPosition = matches.length > 0 && state.searchActiveIndex >= 0
    ? `${state.searchActiveIndex + 1} of ${matches.length}`
    : hasQuery ? 'No results' : '';
  const selectedBook = selectedLibraryBook();
  const completed = Boolean(selectedBook?.completed);
  const selectedFont = readerFontOption(state.readerFontFamily);
  const appearanceButtons: Array<{ mode: ColorMode; label: string }> = [
    { mode: 'system', label: 'System' },
    { mode: 'daylight', label: 'Light' },
    { mode: 'dark', label: 'Dark' },
  ];
  return `
    <header class="app-titlebar" data-tauri-drag-region aria-label="Window toolbar">
      <div class="titlebar-brand" data-tauri-drag-region>
        <span class="preview-traffic-lights" aria-hidden="true">
          <span></span><span></span><span></span>
        </span>
        <img class="titlebar-logo" src="${appLogoUrl}" alt="" aria-hidden="true" />
        <strong data-tauri-drag-region>Goosereader</strong>
      </div>
      <div class="titlebar-controls" aria-label="Reader controls">
        <div class="segmented-control appearance-segmented" role="group" aria-label="Appearance">
          ${appearanceButtons.map(button => `
            <button class="${state.colorMode === button.mode ? 'active' : ''}" type="button" data-color-mode-choice="${button.mode}" aria-pressed="${state.colorMode === button.mode}" title="${button.mode === 'system' ? `Following ${resolvedColorMode}` : `${button.label} appearance`}">
              ${button.label}
            </button>
          `).join('')}
        </div>
        <div class="titlebar-stepper" aria-label="Reader font size">
          <button id="font-size-decrease" type="button" aria-label="Decrease font size">-</button>
          <span aria-hidden="true">Aa</span>
          <button id="font-size-increase" type="button" aria-label="Increase font size">+</button>
        </div>
        <div class="titlebar-font-menu" data-font-menu>
          <button id="reader-font-menu-button" class="titlebar-font-button" type="button" aria-haspopup="listbox" aria-expanded="${state.readerFontMenuOpen}" aria-label="Reader font family">
            <span class="titlebar-font-current">${escapeHtml(selectedFont.label)}</span>
            <span class="titlebar-font-chevron" aria-hidden="true">⌄</span>
          </button>
          ${state.readerFontMenuOpen ? `
            <div class="titlebar-font-options" role="listbox" aria-label="Reader font choices">
              ${readerFontOptions.map(option => `
                <button class="titlebar-font-option ${state.readerFontFamily === option.value ? 'active' : ''}" type="button" role="option" aria-selected="${state.readerFontFamily === option.value}" data-reader-font-choice="${option.value}">
                  <span class="font-option-check" aria-hidden="true">${state.readerFontFamily === option.value ? '✓' : ''}</span>
                  <span class="font-option-copy">
                    <span class="font-option-label">${escapeHtml(option.label)}</span>
                    <span class="font-option-detail">${escapeHtml(option.detail)}</span>
                  </span>
                </button>
              `).join('')}
            </div>
          ` : ''}
        </div>
      </div>
      <div class="titlebar-actions">
        <button id="toggle-completed" class="reader-complete-button titlebar-complete-button" type="button" aria-pressed="${completed}" ${!book ? 'disabled' : ''} title="${completed ? 'Mark as not completed' : 'Mark as completed'}">
          <span aria-hidden="true">${completed ? '✓' : '○'}</span>
          <span>${completed ? 'Completed' : 'Mark Complete'}</span>
        </button>
        <label class="reader-search-box titlebar-search-box" for="reader-search">
          <span class="search-icon" aria-hidden="true">⌕</span>
          <input id="reader-search" type="search" placeholder="${book ? 'Search in book' : 'Search in book'}" value="${escapeHtml(state.readerSearchQuery)}" autocomplete="off" ${!book ? 'disabled' : ''} />
          <span class="titlebar-shortcut" aria-hidden="true">⌘F</span>
        </label>
        <span class="reader-search-count titlebar-search-count" aria-live="polite">${escapeHtml(activeSearchPosition)}</span>
      </div>
    </header>
  `;
}

function bindAppTitlebar(): void {
  const titlebar = document.querySelector<HTMLElement>('.app-titlebar');
  if (!titlebar || !hasTauriRuntime()) return;
  titlebar.addEventListener('mousedown', event => {
    if (event.button !== 0) return;
    const target = event.target;
    if (target instanceof Element && target.closest('button, input, select, textarea, a, label, [data-font-menu]')) return;
    const appWindow = getCurrentWindow();
    if (event.detail === 2) void appWindow.toggleMaximize();
    else void appWindow.startDragging();
  });
}

function toggleColorMode(): void {
  state.colorMode = nextColorMode(state.colorMode);
  applyColorMode(resolveColorMode(state.colorMode));
  try {
    window.localStorage.setItem(colorModeStorageKey, state.colorMode);
  } catch {
    // Ignore storage failures; the in-memory toggle should still work.
  }
  renderShell();
}

function setColorMode(mode: ColorMode | undefined): void {
  if (mode !== 'system' && mode !== 'daylight' && mode !== 'dark') return;
  state.colorMode = mode;
  applyColorMode(resolveColorMode(state.colorMode));
  try {
    window.localStorage.setItem(colorModeStorageKey, state.colorMode);
  } catch {
    // Ignore storage failures; the in-memory toggle should still work.
  }
  renderShell();
}

function adjustReaderFontSize(delta: -1 | 1): void {
  state.readerFontSizePx = clampNumber(state.readerFontSizePx + delta, minReaderFontSizePx, maxReaderFontSizePx);
  persistReaderSettings();
  applyReaderSettings();
  schedulePagedLayout();
}

function toggleReaderFontMenu(): void {
  state.readerFontMenuOpen = !state.readerFontMenuOpen;
  refreshAppTitlebar();
}

function closeReaderFontMenuFromDocument(event: MouseEvent): void {
  if (event.target instanceof Element && event.target.closest('[data-font-menu]')) return;
  if (!state.readerFontMenuOpen) return;
  state.readerFontMenuOpen = false;
  refreshAppTitlebar();
}

function setReaderFontFamily(fontFamily: unknown): void {
  if (!isReaderFontFamily(fontFamily)) return;
  state.readerFontFamily = fontFamily;
  state.readerFontMenuOpen = false;
  persistReaderSettings();
  applyReaderSettings();
  refreshAppTitlebar();
  schedulePagedLayout();
}

function isReaderFontFamily(fontFamily: unknown): fontFamily is ReaderFontFamily {
  return typeof fontFamily === 'string' && readerFontOptions.some(option => option.value === fontFamily);
}

function readerFontOption(fontFamily: ReaderFontFamily): ReaderFontOption {
  return readerFontOptions.find(option => option.value === fontFamily) ?? readerFontOptions[0];
}

function nextColorMode(mode: ColorMode): ColorMode {
  if (mode === 'system') return 'daylight';
  if (mode === 'daylight') return 'dark';
  return 'system';
}

function colorModeIcon(): string {
  if (state.colorMode === 'system') return '◐';
  return state.colorMode === 'dark' ? '☾' : '☀';
}

function colorModeToggleLabel(): string {
  if (state.colorMode === 'system') {
    return `Appearance follows system (${resolveColorMode(state.colorMode)}). Switch to daylight mode`;
  }
  if (state.colorMode === 'daylight') return 'Daylight mode. Switch to dark mode';
  return 'Dark mode. Switch to system appearance';
}

function resolveColorMode(mode: ColorMode): ResolvedColorMode {
  if (mode !== 'system') return mode;
  return nativeResolvedColorMode ?? (systemColorScheme.matches ? 'dark' : 'daylight');
}

function applyColorMode(mode: ResolvedColorMode): void {
  document.documentElement.dataset.colorMode = mode;
}

function resolveNativeTheme(theme: Theme | null): ResolvedColorMode | undefined {
  if (theme === 'dark') return 'dark';
  if (theme === 'light') return 'daylight';
  return undefined;
}

function hasTauriRuntime(): boolean {
  return typeof (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ === 'object';
}

function applyNativeResolvedColorMode(nextMode: ResolvedColorMode | undefined, source: string): void {
  if (!nextMode) return;
  const changed = nextMode !== nativeResolvedColorMode || source !== nativeResolvedColorModeSource;
  nativeResolvedColorMode = nextMode;
  nativeResolvedColorModeSource = source;
  if (!changed) return;
  if (state.colorMode !== 'system') return;
  applyColorMode(resolveColorMode(state.colorMode));
  renderShell();
}

async function bindNativeColorMode(): Promise<void> {
  if (!hasTauriRuntime()) return;

  const refreshCommandAppearance = async (): Promise<boolean> => {
    try {
      const systemAppearance = await invoke<SystemAppearancePayload>('system_appearance');
      applyNativeResolvedColorMode(systemAppearance.color_mode, systemAppearance.source);
      return true;
    } catch {
      return false;
    }
  };

  const appWindow = getCurrentWindow();
  let hasWindowTheme = false;
  try {
    const windowTheme = resolveNativeTheme(await appWindow.theme());
    if (windowTheme) {
      applyNativeResolvedColorMode(windowTheme, 'system window');
      hasWindowTheme = true;
    } else {
      await refreshCommandAppearance();
    }
  } catch {
    await refreshCommandAppearance();
  }

  try {
    await appWindow.onThemeChanged(({ payload }) => {
      applyNativeResolvedColorMode(resolveNativeTheme(payload), 'system window');
    });
  } catch {
    if (!hasWindowTheme) await refreshCommandAppearance();
  }
}

function readStoredColorMode(): ColorMode {
  try {
    const stored = window.localStorage.getItem(colorModeStorageKey);
    return stored === 'daylight' || stored === 'dark' ? stored : 'system';
  } catch {
    return 'system';
  }
}

function readStoredReaderMode(): ReaderMode {
  try {
    return window.localStorage.getItem(readerModeStorageKey) === 'paged' ? 'paged' : 'scroll';
  } catch {
    return 'scroll';
  }
}

function readStoredReaderFontFamily(): ReaderFontFamily {
  try {
    const stored = window.localStorage.getItem(readerFontFamilyStorageKey);
    if (stored === 'serif') return 'iowan';
    if (stored === 'system' || stored === 'system-sans' || stored === 'mono') return 'literata';
    return isReaderFontFamily(stored) ? stored : 'literata';
  } catch {
    return 'literata';
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

function readStoredBoolean(key: string, fallback: boolean): boolean {
  try {
    const stored = window.localStorage.getItem(key);
    if (stored === 'true') return true;
    if (stored === 'false') return false;
    return fallback;
  } catch {
    return fallback;
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

function clampProgressPosition(value: number | null | undefined): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.round(value));
}

function toggleOutlineSidebar(): void {
  state.outlineSidebarHidden = !state.outlineSidebarHidden;
  try {
    window.localStorage.setItem(outlineSidebarHiddenStorageKey, String(state.outlineSidebarHidden));
  } catch {
    // Keep the in-memory state even if persistence is unavailable.
  }
  renderShell();
  schedulePagedLayout();
}

function toggleLibrarySidebar(): void {
  state.librarySidebarHidden = !state.librarySidebarHidden;
  try {
    window.localStorage.setItem(librarySidebarHiddenStorageKey, String(state.librarySidebarHidden));
  } catch {
    // Keep the in-memory state even if persistence is unavailable.
  }
  renderShell();
  schedulePagedLayout();
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
    window.localStorage.setItem(readerFontFamilyStorageKey, state.readerFontFamily);
  } catch {
    // Ignore storage failures; the current layout should still update in memory.
  }
}

function sidebarNavigationContent(): string {
  const isHome = !state.book;
  return `
    <nav class="books-nav" aria-label="Main navigation">
      <label class="search-box" for="library-search">
        <span class="search-icon" aria-hidden="true">⌕</span>
        <input id="library-search" type="search" placeholder="Search" value="${escapeHtml(state.librarySearch)}" autocomplete="off" />
      </label>
      <button id="home-button" class="nav-item ${isHome ? 'active' : ''}" type="button" ${isHome ? 'aria-current="page"' : ''}>
        <span class="nav-icon" aria-hidden="true">⌂</span>
        <span>Home</span>
      </button>
      <button id="all-books-button" class="nav-item" type="button">
        <span class="nav-icon" aria-hidden="true">▥</span>
        <span>All Books</span>
      </button>
    </nav>
  `;
}

function librarySidebarBooksContent(): string {
  const books = filteredLibrary();
  if (state.libraryLoading) {
    return '<section class="sidebar-section sidebar-book-list-panel"><div class="section-heading"><span>Library</span></div><p class="library-empty">Loading books…</p></section>';
  }
  if (state.library.length === 0) {
    return '<section class="sidebar-section sidebar-book-list-panel"><div class="section-heading"><span>Library</span></div><p class="library-empty">Import EPUB files to add them here.</p></section>';
  }
  if (books.length === 0) {
    return '<section class="sidebar-section sidebar-book-list-panel"><div class="section-heading"><span>Library</span></div><p class="library-empty">No matching books.</p></section>';
  }

  const continueBooks = books.filter(book => Boolean(book.progress_chapter_path) && !book.completed).slice(0, 3);
  const libraryBooks = [...books].sort((left, right) => right.modified_ms - left.modified_ms);
  return `
    ${continueBooks.length > 0 ? `
      <section class="sidebar-section sidebar-book-list-panel" aria-label="Continue reading">
        <div class="section-heading"><span>Continue Reading</span></div>
        <div class="sidebar-book-list">
          ${continueBooks.map(sidebarBookItem).join('')}
        </div>
      </section>
    ` : ''}
    <section class="sidebar-section sidebar-book-list-panel sidebar-library-list-panel" aria-label="Imported EPUB library">
      <div class="section-heading">
        <span>Library</span>
        <small>${books.length} book${books.length === 1 ? '' : 's'}</small>
      </div>
      <div class="sidebar-book-list">
        ${libraryBooks.map(sidebarBookItem).join('')}
      </div>
    </section>
  `;
}

function sidebarBookItem(book: LibraryBook): string {
  const percent = normalizedReadingProgressPercent(book);
  const progress = book.completed
    ? 'Completed'
    : percent !== undefined
      ? `${percent}%`
      : `${book.chapter_count} ch`;
  const selected = book.path === state.selectedBookPath;
  return `
    <button class="sidebar-book-item ${selected ? 'active' : ''}" type="button" data-book="${escapeHtml(book.path)}" ${state.loading || Boolean(state.deletingBookPath) ? 'disabled' : ''} ${selected ? 'aria-current="page"' : ''}>
      <span class="sidebar-book-cover">
        ${bookCoverContent(book)}
      </span>
      <span class="sidebar-book-copy">
        <span class="sidebar-book-title">${escapeHtml(state.loading && selected ? 'Opening…' : book.title)}</span>
        <span class="sidebar-book-meta">${escapeHtml(progress)}</span>
        ${percent !== undefined ? `
          <span class="sidebar-progress-meter" aria-label="${percent}% read">
            <span style="width: ${percent}%"></span>
          </span>
        ` : ''}
      </span>
    </button>
  `;
}

function settingsMenuContent(resolvedColorMode: ResolvedColorMode): string {
  if (!state.settingsOpen) return '';
  return `
    <div id="settings-menu" class="settings-menu" role="region" aria-label="Settings">
      <section class="sidebar-section appearance-panel" aria-label="Appearance controls">
        <div class="section-heading"><span>Appearance</span></div>
        <button id="color-mode-toggle" class="appearance-toggle" type="button" aria-pressed="${resolvedColorMode === 'dark'}" aria-label="${colorModeToggleLabel()}" title="${colorModeToggleLabel()}">
          <span class="appearance-toggle-icon" aria-hidden="true">${colorModeIcon()}</span>
          <span>
            <strong>${colorModeTitle()}</strong>
            <small>${colorModeSubtitle()}</small>
          </span>
        </button>
      </section>

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
        <label class="reader-setting" for="reader-mode">
          <span class="reader-setting-label">
            <span>Reading mode</span>
          </span>
          <select id="reader-mode" class="theme-select">
            <option value="scroll" ${state.readerMode === 'scroll' ? 'selected' : ''}>Scroll</option>
            <option value="paged" ${state.readerMode === 'paged' ? 'selected' : ''}>Pages</option>
          </select>
        </label>
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

function colorModeTitle(): string {
  if (state.colorMode === 'system') return 'System';
  return state.colorMode === 'daylight' ? 'Daylight' : 'Dark';
}

function colorModeSubtitle(): string {
  if (state.colorMode === 'system') return `Following ${resolveColorMode(state.colorMode)} (${colorModeSourceLabel()})`;
  return 'Click to change';
}

function colorModeSourceLabel(): string {
  return nativeResolvedColorModeSource || 'browser preview';
}

function libraryContextMenuContent(): string {
  const menu = state.libraryContextMenu;
  if (!menu) return '';
  const left = Math.max(8, Math.min(window.innerWidth - 180, menu.x));
  const top = Math.max(8, Math.min(window.innerHeight - 64, menu.y));
  return `
    <div class="library-context-menu" role="menu" style="left: ${left}px; top: ${top}px">
      <button class="context-menu-item danger" type="button" role="menuitem" data-context-delete-book="${escapeHtml(menu.path)}" data-context-delete-title="${escapeHtml(menu.title)}">
        Delete Book
      </button>
    </div>
  `;
}

function readerToolbarContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  const visibleBooks = filteredLibrary();
  return `
    <header class="reader-toolbar" aria-label="Reader toolbar">
      <div class="reader-toolbar-copy">
        <button class="toolbar-icon-button library-toggle-button" type="button" data-toggle-library-sidebar aria-pressed="${!state.librarySidebarHidden}" aria-label="${state.librarySidebarHidden ? 'Show' : 'Hide'} library sidebar" title="${state.librarySidebarHidden ? 'Show' : 'Hide'} library sidebar">
          <span aria-hidden="true">${state.librarySidebarHidden ? '☰' : '‹'}</span>
        </button>
        ${state.outlineSidebarHidden ? `
          <button class="toolbar-icon-button outline-toggle-button" type="button" data-toggle-outline-sidebar aria-pressed="false" aria-label="Show outline sidebar" title="Show outline sidebar">
            <span aria-hidden="true">☰</span>
          </button>
        ` : ''}
        <span class="reader-toolbar-title">${escapeHtml(book?.title ?? 'Home')}</span>
        <span class="reader-toolbar-subtitle">${escapeHtml(chapter?.title ?? `${visibleBooks.length} book${visibleBooks.length === 1 ? '' : 's'}`)}</span>
      </div>
    </header>
  `;
}

function selectedLibraryBook(): LibraryBook | undefined {
  return state.selectedBookPath ? state.library.find(book => book.path === state.selectedBookPath) : undefined;
}

function filteredLibrary(): LibraryBook[] {
  const query = state.librarySearch.trim().toLocaleLowerCase();
  if (!query) return state.library;
  return state.library.filter(book => {
    const title = book.title.toLocaleLowerCase();
    const fileName = book.file_name.toLocaleLowerCase();
    return title.includes(query) || fileName.includes(query);
  });
}

function currentSearchMatches(): TextSearchMatch[] {
  if (!state.book) return [];
  const query = normalizeQuery(state.readerSearchQuery);
  if (searchCacheBook === state.book && searchCacheQuery === query) return searchCacheMatches;
  searchCacheBook = state.book;
  searchCacheQuery = query;
  searchCacheMatches = findTextMatches(state.book.chapters, query);
  return searchCacheMatches;
}

function updateReaderSearch(query: string, cursor: number): void {
  const previousQuery = normalizeQuery(state.readerSearchQuery);
  state.readerSearchQuery = query;
  const nextQuery = normalizeQuery(query);
  const matches = currentSearchMatches();
  const preferredIndex = previousQuery === nextQuery && state.searchActiveIndex >= 0 ? state.searchActiveIndex : 0;
  state.searchActiveIndex = matches.length > 0 ? clampNumber(preferredIndex, 0, matches.length - 1) : -1;
  state.pendingSearchScroll = matches.length > 0;
  if (matches.length > 0 && state.selectedPath !== matches[state.searchActiveIndex].chapterPath) {
    state.selectedPath = matches[state.searchActiveIndex].chapterPath;
    state.expandedChapterPaths.add(matches[state.searchActiveIndex].chapterPath);
    void saveCurrentReadingProgress();
  }
  renderShell();
  requestAnimationFrame(() => {
    const input = document.querySelector<HTMLInputElement>('#reader-search');
    input?.focus();
    input?.setSelectionRange(cursor, cursor);
  });
}

function clearReaderSearch(): void {
  state.readerSearchQuery = '';
  state.searchActiveIndex = -1;
  state.pendingSearchScroll = false;
  renderShell();
  requestAnimationFrame(() => document.querySelector<HTMLInputElement>('#reader-search')?.focus());
}

function moveSearchResult(offset: 1 | -1): void {
  const matches = currentSearchMatches();
  if (matches.length === 0) return;
  const current = state.searchActiveIndex >= 0 ? state.searchActiveIndex : (offset > 0 ? -1 : 0);
  state.searchActiveIndex = (current + offset + matches.length) % matches.length;
  const match = matches[state.searchActiveIndex];
  state.selectedPath = match.chapterPath;
  state.pendingSearchScroll = true;
  state.expandedChapterPaths.add(match.chapterPath);
  renderShell();
  void saveCurrentReadingProgress();
}

function showHome(): void {
  clearScheduledProgressSave();
  void saveCurrentReadingProgress();
  renderedChapterCache.clear();
  state.book = undefined;
  state.selectedBookPath = undefined;
  state.selectedPath = undefined;
  state.pendingFragment = undefined;
  state.readerSearchQuery = '';
  state.searchActiveIndex = -1;
  state.pendingSearchScroll = false;
  state.expandedChapterPaths = new Set();
  state.chapterListScrollTop = 0;
  state.readerPaneScrollTop = 0;
  state.pendingScrollRestore = false;
  state.error = undefined;
  state.libraryContextMenu = undefined;
  renderShell({ preserveChapterScroll: false, preserveReaderScroll: false });
}

function homeContent(): string {
  const books = filteredLibrary();
  const continueBooks = books.filter(book => Boolean(book.progress_chapter_path) && !book.completed);
  const recentBooks = [...books].sort((left, right) => right.modified_ms - left.modified_ms).slice(0, 10);
  return `
    <section class="home-view" aria-label="Library home">
      <header class="home-header">
        <div>
          <h2>Home</h2>
        </div>
        <div class="home-header-actions">
          <span class="home-count">${books.length} book${books.length === 1 ? '' : 's'}</span>
          ${state.homeDeleteMode ? '<button id="done-delete-mode" class="link-button refresh-home-button" type="button">Done</button>' : ''}
          <button id="refresh-library" class="link-button refresh-home-button" type="button">Refresh</button>
        </div>
      </header>
      ${state.libraryError ? `<p class="inline-error home-error">${escapeHtml(state.libraryError)}</p>` : ''}
      ${homeIntroContent(books)}
      ${bookShelf('Continue', continueBooks, 'Pick up saved books where you left off.', 'continue')}
      ${bookShelf('Recently Added', recentBooks, 'Your newest imports.', 'cover')}
      ${bookShelf('Library', books, 'All local books.', 'cover')}
    </section>
  `;
}

function homeIntroContent(books: LibraryBook[]): string {
  if (state.libraryLoading) {
    return '<div class="home-empty-card">Loading your library…</div>';
  }
  if (state.library.length === 0) {
    return `
      <div class="home-empty-card">
        <strong>Build your bookshelf</strong>
        <span>Import EPUB files to add them here.</span>
      </div>
    `;
  }
  if (books.length === 0) {
    return `
      <div class="home-empty-card">
        <strong>No matching books</strong>
        <span>Clear the sidebar search to show your full library.</span>
      </div>
    `;
  }
  return '';
}

function bookShelf(title: string, books: LibraryBook[], subtitle: string, variant: 'continue' | 'cover'): string {
  if (books.length === 0) return '';
  return `
    <section class="home-section" aria-label="${escapeHtml(title)}">
      <div class="home-section-heading">
        <div>
          <h3>${escapeHtml(title)} <span aria-hidden="true">›</span></h3>
          <p>${escapeHtml(subtitle)}</p>
        </div>
      </div>
      <div class="home-shelf ${variant === 'continue' ? 'continue-shelf' : ''}">
        ${books.map(book => bookCard(book, variant)).join('')}
      </div>
    </section>
  `;
}

function bookCard(book: LibraryBook, variant: 'continue' | 'cover'): string {
  const percent = normalizedReadingProgressPercent(book);
  const progress = book.completed
    ? 'Completed'
    : percent !== undefined
      ? `${percent}% read`
      : book.progress_chapter_path
        ? 'Progress saved'
        : `${book.chapter_count} chapter${book.chapter_count === 1 ? '' : 's'}`;
  return `
    <div class="home-book-card ${variant} ${state.homeDeleteMode ? 'delete-mode' : ''}" data-home-book-card data-menu-book="${escapeHtml(book.path)}" data-menu-title="${escapeHtml(book.title)}">
      <button class="home-book-open ${variant}" type="button" data-book="${escapeHtml(book.path)}" ${state.loading || Boolean(state.deletingBookPath) ? 'disabled' : ''}>
        ${bookCoverContent(book)}
        <span class="home-book-copy">
          <span class="home-book-title">${escapeHtml(state.loading && book.path === state.selectedBookPath ? 'Opening…' : book.title)}</span>
          <span class="home-book-meta">${escapeHtml(progress)}</span>
          ${variant === 'continue' && percent !== undefined ? `
            <span class="home-progress-meter" aria-label="${percent}% read">
              <span style="width: ${percent}%"></span>
            </span>
          ` : ''}
        </span>
      </button>
      ${state.homeDeleteMode ? `
        <button class="home-delete-book-button" type="button" data-delete-book="${escapeHtml(book.path)}" data-delete-title="${escapeHtml(book.title)}" ${state.deletingBookPath ? 'disabled' : ''} aria-label="Delete ${escapeHtml(book.title)}" title="Delete ${escapeHtml(book.title)}">
          ${state.deletingBookPath === book.path ? '…' : '×'}
        </button>
      ` : ''}
    </div>
  `;
}

function normalizedReadingProgressPercent(book: LibraryBook): number | undefined {
  if (book.completed) return 100;
  if (typeof book.reading_progress_percent !== 'number' || Number.isNaN(book.reading_progress_percent)) return undefined;
  return Math.max(1, Math.min(99, Math.round(book.reading_progress_percent)));
}

function selectedBookProgressPercent(): number | null {
  if (!state.book || !state.selectedPath || state.book.chapters.length === 0) return null;
  const chapterIndex = state.book.chapters.findIndex(chapter => chapter.path === state.selectedPath);
  if (chapterIndex < 0) return null;
  return Math.max(1, Math.min(99, Math.round(((chapterIndex + 1) / state.book.chapters.length) * 100)));
}

function bookCoverContent(book: LibraryBook): string {
  if (book.cover_image) {
    return `<span class="image-cover"><img src="${escapeHtml(book.cover_image)}" alt="" loading="lazy" /></span>`;
  }
  return `
    <span class="generated-cover theme-${bookCoverTheme(book)}" aria-hidden="true">
      <span class="cover-title">${escapeHtml(book.title)}</span>
      <span class="cover-mark">zmd</span>
    </span>
  `;
}

function bookCoverTheme(book: LibraryBook): number {
  const source = `${book.title}\u0000${book.file_name}`;
  let hash = 0;
  for (let index = 0; index < source.length; index += 1) {
    hash = (hash * 31 + source.charCodeAt(index)) >>> 0;
  }
  return (hash % 6) + 1;
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
    return homeContent();
  }
  const cached = renderedChapterCache.get(chapterCacheKey(book, chapter));
  const isPaged = state.readerMode === 'paged';
  return `
    <article class="reader-card ${isPaged ? 'paged' : 'scroll'}" style="${readerPreferenceStyle()}" aria-label="Reader chapter. ${isPaged ? 'Use page controls to change pages.' : 'Swipe left or right to change chapters.'}">
      <style>${styleTagContent(scopeReaderCss(book.style_css) + "\n" + scopeReaderCss(state.themeCss ?? '') + "\n" + readerPreferenceCss() + "\n" + readerColorModeCss())}</style>
      <div class="book-content ${isPaged ? 'paged-content' : ''}" data-render-chapter="${escapeHtml(chapter.path)}">${cached ?? loadingChapterMarkup(chapter)}</div>
    </article>
  `;
}

function readerNavigationContent(book: BookPayload | undefined, chapter: BookChapter | undefined): string {
  if (!book || !chapter || state.error) return '';
  const index = book.chapters.findIndex(item => item.path === chapter.path);
  const isPaged = state.readerMode === 'paged';
  return `
    <footer class="reader-bottom-bar" aria-label="Reader navigation">
      <button id="${isPaged ? 'previous-page' : 'previous-chapter'}" type="button" ${isPaged ? previousPageDisabled(index) : index <= 0 ? 'disabled' : ''}>Previous</button>
      <span class="reader-position" aria-live="polite">
        <span id="page-position">${isPaged ? pagePositionText(index, book.chapters.length) : `Chapter ${index + 1} of ${book.chapters.length}`}</span>
        <span class="swipe-hint">${isPaged ? 'Use arrows or swipe to turn pages' : 'Swipe left or right to change chapters'}</span>
      </span>
      <button id="${isPaged ? 'next-page' : 'next-chapter'}" type="button" ${isPaged ? nextPageDisabled(index, book.chapters.length) : index >= book.chapters.length - 1 ? 'disabled' : ''}>Next</button>
    </footer>
  `;
}

function pagePositionText(chapterIndex: number, chapterCount: number): string {
  return `Page ${state.pageIndex + 1} of ${Math.max(1, state.pageCount)} • Chapter ${chapterIndex + 1} of ${chapterCount}`;
}

function previousPageDisabled(chapterIndex: number): string {
  return chapterIndex <= 0 && state.pageIndex <= 0 ? 'disabled' : '';
}

function nextPageDisabled(chapterIndex: number, chapterCount: number): string {
  return chapterIndex >= chapterCount - 1 && state.pageIndex >= Math.max(0, state.pageCount - 1) ? 'disabled' : '';
}

function readerPreferenceStyle(): string {
  return [
    `--reader-content-width: ${state.readerWidthCh}ch`,
    `--reader-font-size: ${state.readerFontSizePx}px`,
    `--reader-font-family: ${readerFontStack(state.readerFontFamily)}`,
    `--reader-heading-font-family: ${readerFontStack(state.readerFontFamily)}`,
  ].join('; ');
}

function readerFontStack(fontFamily: ReaderFontFamily): string {
  if (fontFamily === 'new-york') return '"New York", ui-serif, Georgia, serif';
  if (fontFamily === 'iowan') return '"Iowan Old Style", "Palatino Linotype", Palatino, Charter, "Book Antiqua", Georgia, serif';
  if (fontFamily === 'charter') return 'Charter, "Bitstream Charter", "Iowan Old Style", Georgia, serif';
  if (fontFamily === 'georgia') return 'Georgia, "Times New Roman", Times, serif';
  if (fontFamily === 'palatino') return '"Palatino Linotype", Palatino, "Book Antiqua", "Iowan Old Style", Georgia, serif';
  if (fontFamily === 'baskerville') return 'Baskerville, "Libre Baskerville", Georgia, serif';
  return '"Literata", "Iowan Old Style", "Palatino Linotype", Palatino, Charter, "Book Antiqua", Georgia, serif';
}

function readerPreferenceCss(): string {
  return `
    .reader-card .book-content {
      max-width: var(--reader-content-width) !important;
      font-family: var(--reader-font-family) !important;
      font-size: var(--reader-font-size) !important;
    }

    .reader-card .book-content h1,
    .reader-card .book-content h2,
    .reader-card .book-content h3,
    .reader-card .book-content h4,
    .reader-card .book-content h5,
    .reader-card .book-content h6 {
      font-family: var(--reader-heading-font-family) !important;
    }

    .reader-card .book-content p,
    .reader-card .book-content li,
    .reader-card .book-content blockquote,
    .reader-card .book-content td,
    .reader-card .book-content th,
    .reader-card .book-content span,
    .reader-card .book-content div,
    .reader-card .book-content section,
    .reader-card .book-content article {
      font-family: var(--reader-font-family) !important;
      font-size: inherit !important;
    }
  `;
}

function readerColorModeCss(): string {
  if (resolveColorMode(state.colorMode) !== 'dark') return '';
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
    renderShell({ preserveChapterScroll: false, preserveReaderScroll: false });
  }
}

async function openLibraryBook(path: string): Promise<void> {
  if (state.selectedBookPath !== path) {
    clearScheduledProgressSave();
    void saveCurrentReadingProgress();
  }
  state.loading = true;
  state.error = undefined;
  state.selectedBookPath = path;
  renderShell();
  try {
    const book = await invoke<BookPayload>('load_book_file', { path });
    const progress = await loadReadingProgress(path);
    openBookPayload(book, path, progress);
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    renderShell({ preserveChapterScroll: false, preserveReaderScroll: false });
  }
}

async function deleteLibraryBook(path: string, title: string): Promise<void> {
  state.libraryContextMenu = undefined;
  if (!window.confirm(`Delete "${title}" from your goosereader library? This removes the local .zmd file.`)) {
    renderShell();
    return;
  }
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
      state.readerPaneScrollTop = 0;
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

async function toggleSelectedBookCompleted(): Promise<void> {
  if (!state.selectedBookPath || !state.selectedPath) return;
  const book = selectedLibraryBook();
  const completed = !book?.completed;
  try {
    await invoke<void>('set_book_completed', {
      path: state.selectedBookPath,
      completed,
      chapterPath: state.selectedPath,
    });
    if (book) {
      book.completed = completed;
      book.progress_chapter_path = completed ? null : state.selectedPath;
      book.reading_progress_percent = completed ? null : selectedBookProgressPercent();
    }
    renderShell({ preserveReaderScroll: true });
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
    renderShell({ preserveReaderScroll: true });
  }
}

async function saveCurrentReadingProgress(): Promise<void> {
  if (!state.selectedBookPath || !state.selectedPath) return;
  captureReaderPaneScroll();
  try {
    await invoke<void>('save_reading_progress', {
      path: state.selectedBookPath,
      chapterPath: state.selectedPath,
      pageIndex: state.readerMode === 'paged' ? state.pageIndex : null,
      scrollTop: state.readerMode === 'scroll' ? state.readerPaneScrollTop : null,
    });
    const book = selectedLibraryBook();
    if (book) {
      book.progress_chapter_path = book.completed ? null : state.selectedPath;
      book.reading_progress_percent = book.completed ? null : selectedBookProgressPercent();
    }
  } catch (error) {
    state.libraryError = error instanceof Error ? error.message : String(error);
    renderShell();
  }
}

function scheduleCurrentReadingProgressSave(): void {
  if (progressSaveTimer) window.clearTimeout(progressSaveTimer);
  progressSaveTimer = window.setTimeout(() => {
    progressSaveTimer = undefined;
    void saveCurrentReadingProgress();
  }, 350);
}

function clearScheduledProgressSave(): void {
  if (!progressSaveTimer) return;
  window.clearTimeout(progressSaveTimer);
  progressSaveTimer = undefined;
}

function openBookPayload(book: BookPayload, path: string, progress?: ReadingProgress): void {
  renderedChapterCache.clear();
  searchCacheBook = undefined;
  searchCacheQuery = '';
  searchCacheMatches = [];
  state.book = book;
  state.selectedBookPath = path;
  state.readerSearchQuery = '';
  state.searchActiveIndex = -1;
  state.pendingSearchScroll = false;
  state.readerPaneScrollTop = clampProgressPosition(progress?.scroll_top);
  state.pageIndex = clampProgressPosition(progress?.page_index);
  state.pageCount = 1;
  state.pendingPageTarget = state.pageIndex > 0 ? undefined : 'start';
  state.pendingScrollRestore = state.readerPaneScrollTop > 0;
  const progressChapter = book.chapters.find(chapter => chapter.path === progress?.chapter_path);
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
    applySearchHighlights(target, chapter);
    updatePagedLayout();
    scrollPendingFragment();
    restorePendingReaderScroll();
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
  applySearchHighlights(target, chapter);
  updatePagedLayout();
  scrollPendingFragment();
  restorePendingReaderScroll();
}

function restorePendingReaderScroll(): void {
  if (!state.pendingScrollRestore || state.readerMode !== 'scroll') return;
  const readerScrollContainer = currentReaderScrollContainer();
  if (!readerScrollContainer) return;
  readerScrollContainer.scrollTop = state.readerPaneScrollTop;
  state.pendingScrollRestore = false;
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
  selectChapter(nextPath, { pageTarget: 'start' });
  scrollReaderPaneToTop();
}

function moveReaderPage(offset: ChapterOffset): void {
  if (state.readerMode !== 'paged') {
    moveChapter(offset);
    return;
  }
  if (offset > 0) {
    if (state.pageIndex < state.pageCount - 1) {
      setReaderPage(state.pageIndex + 1);
      return;
    }
    const nextPath = targetChapterPath(state.book?.chapters ?? [], state.selectedPath, 1);
    if (nextPath) selectChapter(nextPath, { pageTarget: 'start' });
    return;
  }
  if (state.pageIndex > 0) {
    setReaderPage(state.pageIndex - 1);
    return;
  }
  const previousPath = targetChapterPath(state.book?.chapters ?? [], state.selectedPath, -1);
  if (previousPath) selectChapter(previousPath, { pageTarget: 'end' });
}

function setReaderPage(pageIndex: number): void {
  state.pageIndex = clampNumber(pageIndex, 0, Math.max(0, state.pageCount - 1));
  applyPagedScroll('smooth');
  updatePageControls();
  scheduleCurrentReadingProgressSave();
}

function selectChapter(chapterPath: string, options: { fragment?: string; pageTarget?: 'start' | 'end' } = {}): void {
  state.selectedPath = chapterPath;
  state.error = undefined;
  state.pendingFragment = options.fragment;
  state.pendingPageTarget = options.fragment ? undefined : options.pageTarget ?? 'start';
  state.pageIndex = 0;
  state.pageCount = 1;
  if (!options.fragment) state.readerPaneScrollTop = 0;
  state.expandedChapterPaths.add(chapterPath);
  renderShell({ preserveReaderScroll: false });
  void saveCurrentReadingProgress();
}

function captureChapterListScroll(): void {
  const chapterList = document.querySelector<HTMLElement>('.chapter-list');
  if (chapterList) state.chapterListScrollTop = chapterList.scrollTop;
}

function captureReaderPaneScroll(): void {
  const readerScrollContainer = currentReaderScrollContainer();
  if (readerScrollContainer) state.readerPaneScrollTop = readerScrollContainer.scrollTop;
}

function bindReaderProgressTracking(): void {
  const readerScrollContainer = currentReaderScrollContainer();
  if (!readerScrollContainer) return;
  readerScrollContainer.addEventListener('scroll', () => {
    if (state.readerMode !== 'scroll') return;
    captureReaderPaneScroll();
    scheduleCurrentReadingProgressSave();
  }, { passive: true });
}

function restoreChapterListScroll(): void {
  const chapterList = document.querySelector<HTMLElement>('.chapter-list');
  if (!chapterList) return;
  chapterList.scrollTop = state.chapterListScrollTop;
}

function restoreReaderPaneScroll(): void {
  const readerScrollContainer = currentReaderScrollContainer();
  if (!readerScrollContainer) return;
  readerScrollContainer.scrollTop = state.readerPaneScrollTop;
}

function currentReaderScrollContainer(): HTMLElement | undefined {
  return document.querySelector<HTMLElement>('.reader-stage')
    ?? document.querySelector<HTMLElement>('.reader-pane')
    ?? undefined;
}

function bindSidebarResize(): void {
  const handle = document.querySelector<HTMLElement>('#sidebar-resize-handle');
  const shell = document.querySelector<HTMLElement>('.shell');
  const outline = document.querySelector<HTMLElement>('.outline-sidebar');
  if (!handle || !shell || !outline) return;

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
      const outlineLeft = outline.getBoundingClientRect().left;
      applyWidth(clientX - outlineLeft);
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

function bindHomeBookLongPress(): void {
  clearHomeLongPressTimer();
  document.querySelectorAll<HTMLButtonElement>('.home-book-open[data-book]').forEach(button => {
    let startX = 0;
    let startY = 0;
    const cancelLongPress = (): void => clearHomeLongPressTimer();

    button.addEventListener('pointerdown', event => {
      lastHomeBookPointerType = event.pointerType;
      lastHomeBookPointerAt = Date.now();
      if (!event.isPrimary || event.pointerType === 'mouse' || state.homeDeleteMode) return;
      startX = event.clientX;
      startY = event.clientY;
      clearHomeLongPressTimer();
      homeLongPressTimer = window.setTimeout(() => {
        enterHomeDeleteMode();
      }, 620);
    });

    button.addEventListener('pointermove', event => {
      if (!homeLongPressTimer) return;
      const moved = Math.hypot(event.clientX - startX, event.clientY - startY);
      if (moved > 10) cancelLongPress();
    });
    button.addEventListener('pointerup', cancelLongPress);
    button.addEventListener('pointercancel', cancelLongPress);
    button.addEventListener('pointerleave', cancelLongPress);
  });
}

function enterHomeDeleteMode(): void {
  clearHomeLongPressTimer();
  suppressHomeBookOpenUntil = Date.now() + 800;
  state.libraryContextMenu = undefined;
  state.homeDeleteMode = true;
  renderShell();
}

function clearHomeLongPressTimer(): void {
  if (!homeLongPressTimer) return;
  window.clearTimeout(homeLongPressTimer);
  homeLongPressTimer = undefined;
}

function bindLibraryContextMenuDismiss(): void {
  if (!state.libraryContextMenu) return;
  document.querySelector<HTMLElement>('.shell')?.addEventListener('click', event => {
    const target = event.target;
    if (!(target instanceof Element) || target.closest('.library-context-menu')) return;
    state.libraryContextMenu = undefined;
    renderShell();
  });
}

function bindReaderSettings(): void {
  const modeInput = document.querySelector<HTMLSelectElement>('#reader-mode');
  const widthInput = document.querySelector<HTMLInputElement>('#reader-width');
  const widthValue = document.querySelector<HTMLOutputElement>('#reader-width-value');
  const fontSizeInput = document.querySelector<HTMLInputElement>('#reader-font-size');
  const fontSizeValue = document.querySelector<HTMLOutputElement>('#reader-font-size-value');

  modeInput?.addEventListener('change', () => {
    state.readerMode = modeInput.value === 'paged' ? 'paged' : 'scroll';
    state.pageIndex = 0;
    state.pageCount = 1;
    state.pendingPageTarget = 'start';
    try {
      window.localStorage.setItem(readerModeStorageKey, state.readerMode);
    } catch {
      // Ignore storage failures; mode can still update in memory.
    }
    renderShell({ preserveReaderScroll: false });
  });

  widthInput?.addEventListener('input', () => {
    state.readerWidthCh = clampNumber(Number(widthInput.value), minReaderWidthCh, maxReaderWidthCh);
    widthInput.value = String(state.readerWidthCh);
    if (widthValue) widthValue.value = `${state.readerWidthCh}ch`;
    applyReaderSettings();
    schedulePagedLayout();
  });
  widthInput?.addEventListener('change', persistReaderSettings);

  fontSizeInput?.addEventListener('input', () => {
    state.readerFontSizePx = clampNumber(Number(fontSizeInput.value), minReaderFontSizePx, maxReaderFontSizePx);
    fontSizeInput.value = String(state.readerFontSizePx);
    if (fontSizeValue) fontSizeValue.value = `${state.readerFontSizePx}px`;
    applyReaderSettings();
    schedulePagedLayout();
  });
  fontSizeInput?.addEventListener('change', persistReaderSettings);
}

function applyReaderSettings(): void {
  document.querySelector<HTMLElement>('.reader-card')?.setAttribute('style', readerPreferenceStyle());
  document.querySelector<HTMLElement>('.book-content')?.style.setProperty('font-family', readerFontStack(state.readerFontFamily), 'important');
}

function schedulePagedLayout(): void {
  if (pagedLayoutTimer) window.clearTimeout(pagedLayoutTimer);
  pagedLayoutTimer = window.setTimeout(() => {
    pagedLayoutTimer = undefined;
    updatePagedLayout();
  }, 80);
}

function updatePagedLayout(): void {
  if (state.readerMode !== 'paged') return;
  requestAnimationFrame(() => {
    const content = currentRenderedBookContent();
    if (!content) return;
    const pageWidth = syncPagedPageWidth(content);
    const pageCount = Math.max(1, Math.ceil(content.scrollWidth / pageWidth));
    state.pageCount = pageCount;
    if (state.pendingPageTarget === 'end') {
      state.pageIndex = pageCount - 1;
    } else {
      state.pageIndex = clampNumber(state.pageIndex, 0, pageCount - 1);
    }
    state.pendingPageTarget = undefined;
    applyPagedScroll('auto');
    updatePageControls();
  });
}

function currentRenderedBookContent(): HTMLElement | undefined {
  const chapter = currentChapter();
  if (!chapter) return undefined;
  return document.querySelector<HTMLElement>(`.book-content[data-render-chapter="${cssString(chapter.path)}"]`) ?? undefined;
}

function syncPagedPageWidth(content: HTMLElement): number {
  const pageWidth = Math.max(1, content.clientWidth);
  content.style.setProperty('--paged-page-width', `${pageWidth}px`);
  return pageWidth;
}

function applyPagedScroll(behavior: ScrollBehavior): void {
  const content = currentRenderedBookContent();
  if (!content || state.readerMode !== 'paged') return;
  const left = state.pageIndex * syncPagedPageWidth(content);
  if (behavior === 'auto') {
    const previousScrollBehavior = content.style.getPropertyValue('scroll-behavior');
    content.style.setProperty('scroll-behavior', 'auto');
    content.scrollTo({ left, behavior: 'auto' });
    requestAnimationFrame(() => {
      if (previousScrollBehavior) {
        content.style.setProperty('scroll-behavior', previousScrollBehavior);
      } else {
        content.style.removeProperty('scroll-behavior');
      }
    });
    return;
  }
  content.scrollTo({ left, behavior });
}

function pageIndexForPagedTarget(target: HTMLElement, content: HTMLElement): number {
  const pageWidth = syncPagedPageWidth(content);
  state.pageCount = Math.max(1, Math.ceil(content.scrollWidth / pageWidth));
  const targetLeft = target.getBoundingClientRect().left - content.getBoundingClientRect().left + content.scrollLeft;
  return clampNumber(
    Math.floor(Math.max(0, targetLeft) / Math.max(1, pageWidth)),
    0,
    Math.max(0, state.pageCount - 1),
  );
}

function updatePageControls(): void {
  if (state.readerMode !== 'paged') return;
  const book = state.book;
  const chapter = currentChapter();
  if (!book || !chapter) return;
  const chapterIndex = book.chapters.findIndex(item => item.path === chapter.path);
  const position = document.querySelector<HTMLElement>('#page-position');
  if (position) position.textContent = pagePositionText(chapterIndex, book.chapters.length);
  const previous = document.querySelector<HTMLButtonElement>('#previous-page');
  const next = document.querySelector<HTMLButtonElement>('#next-page');
  if (previous) previous.disabled = chapterIndex <= 0 && state.pageIndex <= 0;
  if (next) next.disabled = chapterIndex >= book.chapters.length - 1 && state.pageIndex >= state.pageCount - 1;
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
    if (state.readerMode === 'paged') moveReaderPage(decision);
    else moveChapter(decision);
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
    if (state.readerMode === 'paged') moveReaderPage(decision);
    else moveChapter(decision);
  }, { passive: false });
}

function scrollReaderPaneToTop(): void {
  requestAnimationFrame(() => {
    currentReaderScrollContainer()?.scrollTo({ top: 0, behavior: 'smooth' });
  });
}

function handleReaderLink(event: MouseEvent, anchor: HTMLAnchorElement): void {
  const href = anchor.getAttribute('href') ?? '';
  if (isExternalReaderLink(href)) return;
  event.preventDefault();
  const [targetPath, fragment] = href.split('#');
  const current = currentChapter();
  const resolved = targetPath ? resolveBookPath(targetPath, current?.path ?? '') : current?.path;
  const target = findChapterByPath(resolved);
  if (!target) return;
  selectChapter(target.path, { fragment });
}

function isExternalReaderLink(href: string): boolean {
  return /^https?:\/\//i.test(href) || /^mailto:/i.test(href);
}

function findChapterByPath(path: string | undefined): BookChapter | undefined {
  if (!path) return undefined;
  return state.book?.chapters.find(chapter => chapter.path === path || chapter.path === decodeUriPath(path));
}

function decodeUriPath(path: string): string {
  try {
    return decodeURI(path);
  } catch {
    return path;
  }
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

function applySearchHighlights(scope: HTMLElement, chapter: BookChapter): void {
  const query = normalizeQuery(state.readerSearchQuery);
  if (!query) return;

  const matches = currentSearchMatches();
  const active = matches[state.searchActiveIndex];
  const activeOccurrence = active?.chapterPath === chapter.path ? active.matchIndexInChapter : -1;
  let occurrence = 0;
  let activeMark: HTMLElement | undefined;
  const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const text = node.textContent ?? '';
      if (!text.trim()) return NodeFilter.FILTER_REJECT;
      const parent = node.parentElement;
      if (!parent || parent.closest('script, style, mark.reader-search-hit')) return NodeFilter.FILTER_REJECT;
      return text.toLocaleLowerCase().includes(query) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    },
  });
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);

  for (const node of nodes) {
    const text = node.textContent ?? '';
    const lower = text.toLocaleLowerCase();
    const fragment = document.createDocumentFragment();
    let cursor = 0;
    while (cursor < text.length) {
      const found = lower.indexOf(query, cursor);
      if (found === -1) break;
      if (found > cursor) fragment.append(document.createTextNode(text.slice(cursor, found)));
      const mark = document.createElement('mark');
      mark.className = 'reader-search-hit';
      if (occurrence === activeOccurrence) {
        mark.classList.add('active');
        activeMark = mark;
      }
      mark.textContent = text.slice(found, found + query.length);
      fragment.append(mark);
      occurrence += 1;
      cursor = found + query.length;
    }
    if (cursor < text.length) fragment.append(document.createTextNode(text.slice(cursor)));
    node.replaceWith(fragment);
  }

  if (state.pendingSearchScroll && activeMark) {
    if (state.readerMode === 'paged') {
      const content = currentRenderedBookContent();
      if (content) {
        state.pageIndex = pageIndexForPagedTarget(activeMark, content);
        applyPagedScroll('smooth');
        updatePageControls();
        scheduleCurrentReadingProgressSave();
      }
    } else {
      activeMark.scrollIntoView({ block: 'center', behavior: 'smooth' });
    }
    state.pendingSearchScroll = false;
  }
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

  if (state.readerMode === 'paged') {
    const content = currentRenderedBookContent();
    if (content) {
      state.pageIndex = pageIndexForPagedTarget(target, content);
      applyPagedScroll('smooth');
      updatePageControls();
      scheduleCurrentReadingProgressSave();
    }
  } else {
    const readerScrollContainer = currentReaderScrollContainer();
    if (readerScrollContainer) {
      const paneTop = readerScrollContainer.getBoundingClientRect().top;
      const targetTop = target.getBoundingClientRect().top;
      readerScrollContainer.scrollTo({
        top: readerScrollContainer.scrollTop + targetTop - paneTop - 18,
        behavior: 'smooth',
      });
    } else {
      target.scrollIntoView({ block: 'start' });
    }
  }
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

document.addEventListener('keydown', event => {
  if (!state.book || event.defaultPrevented) return;
  if ((event.metaKey || event.ctrlKey) && event.key.toLocaleLowerCase() === 'f') {
    event.preventDefault();
    document.querySelector<HTMLInputElement>('#reader-search')?.focus();
    return;
  }
  const target = event.target;
  if (target instanceof HTMLElement && target.closest('input, textarea, select, button')) return;
  if (state.readerMode === 'paged' && (event.key === 'ArrowRight' || event.key === 'PageDown' || event.key === ' ')) {
    event.preventDefault();
    moveReaderPage(1);
  } else if (state.readerMode === 'paged' && (event.key === 'ArrowLeft' || event.key === 'PageUp')) {
    event.preventDefault();
    moveReaderPage(-1);
  }
});

window.addEventListener('resize', schedulePagedLayout);
systemColorScheme.addEventListener('change', () => {
  if (state.colorMode !== 'system') return;
  applyColorMode(resolveColorMode(state.colorMode));
  renderShell();
});

renderShell();
void bindNativeColorMode();
void bootstrap();
