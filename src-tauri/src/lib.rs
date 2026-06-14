use base64::{engine::general_purpose, Engine as _};
use epubmd_core::convert_epub_to_zip;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};
use zip::ZipArchive;

#[derive(Serialize)]
struct AppPaths {
    books_dir: String,
    themes_dir: String,
}

#[derive(Serialize, Clone)]
struct LibraryBook {
    path: String,
    file_name: String,
    title: String,
    chapter_count: usize,
    progress_chapter_path: Option<String>,
    reading_progress_percent: Option<u8>,
    completed: bool,
    modified_ms: u64,
    cover_image: Option<String>,
    #[serde(skip)]
    chapter_paths: Vec<String>,
}

#[derive(Serialize, Clone)]
struct ThemeEntry {
    path: String,
    file_name: String,
    name: String,
}

#[derive(Serialize)]
struct ImportPayload {
    book: LibraryBook,
    payload: BookPayload,
}

#[derive(Serialize)]
struct BookPayload {
    title: String,
    readme: String,
    style_css: String,
    chapters: Vec<BookChapter>,
    assets: Vec<BookAsset>,
    cover_image: Option<String>,
}

#[derive(Serialize)]
struct BookChapter {
    path: String,
    title: String,
    markdown: String,
}

#[derive(Serialize)]
struct BookAsset {
    path: String,
    data_url: String,
}

#[derive(Deserialize, Default)]
struct BookMetadata {
    cover_asset_path: Option<String>,
}

#[derive(Serialize)]
struct ThemePayload {
    name: String,
    css: String,
}

#[derive(Serialize)]
struct SystemAppearance {
    color_mode: &'static str,
    source: &'static str,
}

#[derive(Serialize, Deserialize, Clone)]
struct ReadingProgress {
    chapter_path: String,
    updated_ms: u64,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    page_index: Option<u32>,
    #[serde(default)]
    scroll_top: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
struct ReadingProgressStore {
    books: HashMap<String, ReadingProgress>,
}

#[tauri::command]
fn app_paths() -> Result<AppPaths, String> {
    let books_dir = ensure_books_dir()?;
    let themes_dir = ensure_themes_dir()?;
    Ok(AppPaths {
        books_dir: display_path(&books_dir),
        themes_dir: display_path(&themes_dir),
    })
}

#[tauri::command]
fn list_books() -> Result<Vec<LibraryBook>, String> {
    list_books_in_dir(&ensure_books_dir()?)
}

#[tauri::command]
fn import_epub(path: String) -> Result<ImportPayload, String> {
    let epub_path = Path::new(&path);
    require_extension(epub_path, &["epub"], "input file must be an .epub file")?;
    let destination = unique_import_destination(epub_path, &ensure_books_dir()?)?;
    let written = convert_epub_to_zip(epub_path, &destination, false)
        .map_err(|error| format!("goosereader: {error}"))?;
    let book = summarize_book(&written)?;
    let payload = load_book_archive(&written)?;
    Ok(ImportPayload { book, payload })
}

#[tauri::command]
fn load_book_file(path: String) -> Result<BookPayload, String> {
    load_book_file_in_dir(Path::new(&path), &ensure_books_dir()?)
}

#[tauri::command]
fn load_book_zip(path: String) -> Result<BookPayload, String> {
    load_book_file(path)
}

#[tauri::command]
fn list_themes() -> Result<Vec<ThemeEntry>, String> {
    list_themes_in_dir(&ensure_themes_dir()?)
}

#[tauri::command]
fn load_theme_file(path: String) -> Result<ThemePayload, String> {
    load_theme_css(path)
}

#[tauri::command]
fn load_theme_css(path: String) -> Result<ThemePayload, String> {
    load_theme_css_in_dir(Path::new(&path), &ensure_themes_dir()?)
}

#[tauri::command]
fn load_reading_progress(path: String) -> Result<Option<ReadingProgress>, String> {
    let books_dir = ensure_books_dir()?;
    load_reading_progress_in_dir(Path::new(&path), &books_dir, &progress_store_path()?)
}

#[tauri::command]
fn save_reading_progress(
    path: String,
    chapter_path: String,
    page_index: Option<u32>,
    scroll_top: Option<u32>,
) -> Result<(), String> {
    let books_dir = ensure_books_dir()?;
    save_reading_progress_in_dir(
        Path::new(&path),
        &chapter_path,
        page_index,
        scroll_top,
        &books_dir,
        &progress_store_path()?,
    )
}

#[tauri::command]
fn set_book_completed(
    path: String,
    completed: bool,
    chapter_path: Option<String>,
) -> Result<(), String> {
    let books_dir = ensure_books_dir()?;
    set_book_completed_in_dir(
        Path::new(&path),
        completed,
        chapter_path.as_deref(),
        &books_dir,
        &progress_store_path()?,
    )
}

#[tauri::command]
fn delete_book(path: String) -> Result<(), String> {
    let books_dir = ensure_books_dir()?;
    delete_book_in_dir(Path::new(&path), &books_dir, &progress_store_path()?)
}

#[tauri::command]
fn system_appearance() -> SystemAppearance {
    platform_system_appearance().unwrap_or(SystemAppearance {
        color_mode: "daylight",
        source: "fallback",
    })
}

fn appearance_from_dark_flag(is_dark: bool, source: &'static str) -> SystemAppearance {
    SystemAppearance {
        color_mode: if is_dark { "dark" } else { "daylight" },
        source,
    }
}

#[cfg(target_os = "macos")]
fn platform_system_appearance() -> Option<SystemAppearance> {
    let script =
        "tell application \"System Events\" to tell appearance preferences to get dark mode";
    if let Ok(output) = Command::new("osascript").args(["-e", script]).output() {
        if output.status.success() {
            if let Some(is_dark) = parse_bool_command_output(&output.stdout) {
                return Some(appearance_from_dark_flag(is_dark, "macOS command"));
            }
        }
    }

    let output = Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()?;
    if !output.status.success() {
        return Some(appearance_from_dark_flag(false, "macOS command"));
    }
    let value = String::from_utf8_lossy(&output.stdout);
    Some(appearance_from_dark_flag(
        value.trim().eq_ignore_ascii_case("dark"),
        "macOS command",
    ))
}

#[cfg(not(target_os = "macos"))]
fn platform_system_appearance() -> Option<SystemAppearance> {
    None
}

fn parse_bool_command_output(output: &[u8]) -> Option<bool> {
    match String::from_utf8_lossy(output)
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn load_reading_progress_in_dir(
    path: &Path,
    books_dir: &Path,
    progress_path: &Path,
) -> Result<Option<ReadingProgress>, String> {
    let book_path = confined_existing_book(path, books_dir)?;
    Ok(read_progress_store_from(progress_path)?
        .books
        .get(&progress_key(&book_path))
        .cloned())
}

fn save_reading_progress_in_dir(
    path: &Path,
    chapter_path: &str,
    page_index: Option<u32>,
    scroll_top: Option<u32>,
    books_dir: &Path,
    progress_path: &Path,
) -> Result<(), String> {
    if chapter_path.trim().is_empty() {
        return Err("chapter path must not be empty".into());
    }
    let book_path = confined_existing_book(path, books_dir)?;
    let mut store = read_progress_store_from(progress_path)?;
    let key = progress_key(&book_path);
    let completed = store
        .books
        .get(&key)
        .map(|progress| progress.completed)
        .unwrap_or(false);
    store.books.insert(
        key,
        ReadingProgress {
            chapter_path: chapter_path.to_string(),
            updated_ms: now_ms(),
            completed,
            page_index,
            scroll_top,
        },
    );
    write_progress_store_to(progress_path, &store)
}

fn set_book_completed_in_dir(
    path: &Path,
    completed: bool,
    chapter_path: Option<&str>,
    books_dir: &Path,
    progress_path: &Path,
) -> Result<(), String> {
    let book_path = confined_existing_book(path, books_dir)?;
    let mut store = read_progress_store_from(progress_path)?;
    let key = progress_key(&book_path);
    let existing = store.books.get(&key).cloned();
    let next_chapter_path = chapter_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            existing
                .as_ref()
                .map(|progress| progress.chapter_path.clone())
        })
        .ok_or_else(|| "chapter path must not be empty".to_string())?;
    store.books.insert(
        key,
        ReadingProgress {
            chapter_path: next_chapter_path,
            updated_ms: now_ms(),
            completed,
            page_index: existing.as_ref().and_then(|progress| progress.page_index),
            scroll_top: existing.as_ref().and_then(|progress| progress.scroll_top),
        },
    );
    write_progress_store_to(progress_path, &store)
}

fn delete_book_in_dir(path: &Path, books_dir: &Path, progress_path: &Path) -> Result<(), String> {
    let book_path = confined_existing_book(path, books_dir)?;
    let mut store = read_progress_store_from(progress_path).unwrap_or_default();
    store.books.remove(&progress_key(&book_path));
    fs::remove_file(&book_path)
        .map_err(|error| format!("could not delete book {}: {error}", book_path.display()))?;
    write_progress_store_to(progress_path, &store)
}

fn confined_existing_book(path: &Path, books_dir: &Path) -> Result<PathBuf, String> {
    let book_path = confined_existing_file(path, books_dir, "book")?;
    require_extension(
        &book_path,
        &["zmd"],
        "book file must be a .zmd archive in the library",
    )?;
    Ok(book_path)
}

fn load_book_file_in_dir(path: &Path, books_dir: &Path) -> Result<BookPayload, String> {
    let path = confined_existing_file(path, books_dir, "book")?;
    require_extension(
        &path,
        &["zmd", "zip"],
        "book file must be a .zmd or legacy .zip archive",
    )?;
    load_book_archive(&path)
}

fn load_theme_css_in_dir(path: &Path, themes_dir: &Path) -> Result<ThemePayload, String> {
    let path = confined_existing_file(path, themes_dir, "theme")?;
    require_extension(&path, &["css"], "theme file must be a .css file")?;

    let css =
        fs::read_to_string(&path).map_err(|error| format!("could not read theme CSS: {error}"))?;
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Custom theme")
        .to_string();
    Ok(ThemePayload { name, css })
}

fn load_book_archive(path: &Path) -> Result<BookPayload, String> {
    let file = File::open(path).map_err(|error| format!("could not open book archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("invalid book archive: {error}"))?;

    let mut readme = String::new();
    let mut style_css = String::new();
    let mut chapters = Vec::new();
    let mut assets = Vec::new();
    let mut metadata = BookMetadata::default();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("could not read archive entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let entry_path = normalize_zip_path(entry.name());
        if entry_path.is_empty() || entry_path.contains("../") {
            continue;
        }

        if entry_path == "README.md" {
            readme = read_utf8_entry(&mut entry, &entry_path)?;
        } else if entry_path == "metadata.json" {
            let text = read_utf8_entry(&mut entry, &entry_path)?;
            metadata = serde_json::from_str(&text).unwrap_or_default();
        } else if entry_path == "style.css" {
            style_css = read_utf8_entry(&mut entry, &entry_path)?;
        } else if entry_path.starts_with("chapters/") && entry_path.ends_with(".md") {
            let markdown = read_utf8_entry(&mut entry, &entry_path)?;
            chapters.push(BookChapter {
                title: chapter_title(&markdown, &entry_path),
                path: entry_path,
                markdown,
            });
        } else if entry_path.starts_with("assets/") {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| format!("could not read asset {entry_path}: {error}"))?;
            let mime = mime_guess::from_path(&entry_path).first_or_octet_stream();
            let encoded = general_purpose::STANDARD.encode(bytes);
            assets.push(BookAsset {
                path: entry_path,
                data_url: format!("data:{mime};base64,{encoded}"),
            });
        }
    }

    order_chapters_from_readme(&mut chapters, &readme);
    assets.sort_by(|left, right| left.path.cmp(&right.path));
    let cover_image = cover_image_from_assets(&assets, metadata.cover_asset_path.as_deref());

    if chapters.is_empty() {
        return Err("book archive does not contain chapters/*.md files".into());
    }

    let title = title_from_readme(&readme)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "goosereader book".to_string());

    Ok(BookPayload {
        title,
        readme,
        style_css,
        chapters,
        assets,
        cover_image,
    })
}

fn list_books_in_dir(books_dir: &Path) -> Result<Vec<LibraryBook>, String> {
    let progress = read_progress_store().unwrap_or_default();
    list_books_in_dir_with_progress(books_dir, &progress)
}

fn list_books_in_dir_with_progress(
    books_dir: &Path,
    progress: &ReadingProgressStore,
) -> Result<Vec<LibraryBook>, String> {
    let mut books = Vec::new();
    for entry in fs::read_dir(books_dir)
        .map_err(|error| format!("could not read books directory: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("could not read books directory entry: {error}"))?
            .path();
        if !has_extension(&path, &["zmd"]) {
            continue;
        }
        match summarize_book(&path) {
            Ok(mut book) => {
                book.progress_chapter_path = progress_for_path(&progress, &path);
                book.reading_progress_percent =
                    progress_percent_for_path(&progress, &path, &book.chapter_paths);
                book.completed = completed_for_path(&progress, &path);
                books.push(book);
            }
            Err(_) => {
                let mut book = fallback_library_book(&path);
                book.progress_chapter_path = progress_for_path(&progress, &path);
                book.reading_progress_percent = None;
                book.completed = completed_for_path(&progress, &path);
                books.push(book);
            }
        }
    }
    books.sort_by(|left, right| {
        right
            .modified_ms
            .cmp(&left.modified_ms)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    Ok(books)
}

fn list_themes_in_dir(themes_dir: &Path) -> Result<Vec<ThemeEntry>, String> {
    let mut themes = Vec::new();
    for entry in fs::read_dir(themes_dir)
        .map_err(|error| format!("could not read themes directory: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("could not read themes directory entry: {error}"))?
            .path();
        if !has_extension(&path, &["css"]) {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("theme.css")
            .to_string();
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(&file_name)
            .to_string();
        themes.push(ThemeEntry {
            path: display_path(&path),
            file_name,
            name,
        });
    }
    themes.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(themes)
}

fn summarize_book(path: &Path) -> Result<LibraryBook, String> {
    let file = File::open(path).map_err(|error| format!("could not open book: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("invalid book archive: {error}"))?;
    let mut readme = String::new();
    let mut chapter_paths = Vec::new();
    let mut metadata = BookMetadata::default();
    let mut asset_paths = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("could not read book entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let entry_path = normalize_zip_path(entry.name());
        if entry_path == "README.md" {
            readme = read_utf8_entry(&mut entry, &entry_path)?;
        } else if entry_path == "metadata.json" {
            let text = read_utf8_entry(&mut entry, &entry_path)?;
            metadata = serde_json::from_str(&text).unwrap_or_default();
        } else if entry_path.starts_with("chapters/") && entry_path.ends_with(".md") {
            chapter_paths.push(entry_path);
        } else if entry_path.starts_with("assets/") && is_image_path(&entry_path) {
            asset_paths.push(entry_path);
        }
    }

    order_chapter_paths_from_readme(&mut chapter_paths, &readme);

    let mut book = fallback_library_book(path);
    book.title = title_from_readme(&readme).unwrap_or(book.title);
    book.chapter_count = chapter_paths.len();
    book.chapter_paths = chapter_paths;
    book.cover_image = choose_cover_asset_path(&asset_paths, metadata.cover_asset_path.as_deref())
        .and_then(|cover_path| read_asset_data_url_from_archive(&mut archive, &cover_path).ok());
    Ok(book)
}

fn fallback_library_book(path: &Path) -> LibraryBook {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("book.zmd")
        .to_string();
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&file_name)
        .to_string();
    LibraryBook {
        path: display_path(path),
        file_name,
        title,
        chapter_count: 0,
        progress_chapter_path: None,
        reading_progress_percent: None,
        completed: false,
        modified_ms: modified_ms(path),
        cover_image: None,
        chapter_paths: Vec::new(),
    }
}

fn read_progress_store() -> Result<ReadingProgressStore, String> {
    read_progress_store_from(&progress_store_path()?)
}

fn read_progress_store_from(path: &Path) -> Result<ReadingProgressStore, String> {
    if !path.exists() {
        return Ok(ReadingProgressStore::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read reading progress: {error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("could not parse reading progress: {error}"))
}

fn write_progress_store_to(path: &Path, store: &ReadingProgressStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "could not create reading progress directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let mut file =
        File::create(path).map_err(|error| format!("could not write reading progress: {error}"))?;
    serde_json::to_writer_pretty(&mut file, store)
        .map_err(|error| format!("could not serialize reading progress: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("could not finish reading progress: {error}"))
}

fn progress_store_path() -> Result<PathBuf, String> {
    Ok(config_root()?.join("reading-progress.json"))
}

fn progress_for_path(store: &ReadingProgressStore, path: &Path) -> Option<String> {
    progress_entry_for_path(store, path)
        .filter(|progress| !progress.completed)
        .map(|progress| progress.chapter_path.clone())
}

fn progress_percent_for_path(
    store: &ReadingProgressStore,
    path: &Path,
    chapter_paths: &[String],
) -> Option<u8> {
    let progress = progress_entry_for_path(store, path)?;
    if progress.completed || chapter_paths.is_empty() {
        return None;
    }
    let chapter_index = chapter_paths
        .iter()
        .position(|chapter_path| chapter_path == &progress.chapter_path)?;
    let percent = (((chapter_index + 1) as f64 / chapter_paths.len() as f64) * 100.0).round() as u8;
    Some(percent.clamp(1, 99))
}

fn completed_for_path(store: &ReadingProgressStore, path: &Path) -> bool {
    progress_entry_for_path(store, path)
        .map(|progress| progress.completed)
        .unwrap_or(false)
}

fn progress_entry_for_path<'a>(
    store: &'a ReadingProgressStore,
    path: &Path,
) -> Option<&'a ReadingProgress> {
    let key = path
        .canonicalize()
        .map(|path| progress_key(&path))
        .unwrap_or_else(|_| progress_key(path));
    store.books.get(&key)
}

fn progress_key(path: &Path) -> String {
    display_path(path)
}

fn unique_import_destination(epub_path: &Path, books_dir: &Path) -> Result<PathBuf, String> {
    let stem = epub_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("book");
    let stem = sanitize_file_stem(stem);
    let mut candidate = books_dir.join(format!("{stem}.zmd"));
    let mut index = 2;
    while candidate.exists() {
        candidate = books_dir.join(format!("{stem}-{index}.zmd"));
        index += 1;
    }
    Ok(candidate)
}

fn sanitize_file_stem(input: &str) -> String {
    let mut value = String::new();
    let mut last_was_dash = false;
    for character in input.chars() {
        let safe = character.is_alphanumeric() || matches!(character, '-' | '_' | '.');
        if safe {
            value.push(character);
            last_was_dash = false;
        } else if !last_was_dash {
            value.push('-');
            last_was_dash = true;
        }
    }
    let value = value.trim_matches(|character| character == '-' || character == '.');
    if value.is_empty() {
        "book".to_string()
    } else {
        value.to_string()
    }
}

fn ensure_books_dir() -> Result<PathBuf, String> {
    ensure_dir(config_root()?.join("books"))
}

fn ensure_themes_dir() -> Result<PathBuf, String> {
    ensure_dir(config_root()?.join("themes"))
}

fn ensure_dir(path: PathBuf) -> Result<PathBuf, String> {
    fs::create_dir_all(&path)
        .map_err(|error| format!("could not create app directory {}: {error}", path.display()))?;
    Ok(path)
}

fn config_root() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("GOOSEREADER_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| "could not locate home directory for ~/.config/goosereader".to_string())?;
    Ok(PathBuf::from(home).join(".config").join("goosereader"))
}

fn title_from_readme(readme: &str) -> Option<String> {
    readme
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn read_utf8_entry<R: Read>(reader: &mut R, path: &str) -> Result<String, String> {
    let mut text = String::new();
    reader
        .read_to_string(&mut text)
        .map_err(|error| format!("{path} is not valid UTF-8: {error}"))?;
    Ok(text)
}

fn cover_image_from_assets(assets: &[BookAsset], metadata_path: Option<&str>) -> Option<String> {
    let asset_paths = assets
        .iter()
        .map(|asset| asset.path.clone())
        .collect::<Vec<_>>();
    let cover_path = choose_cover_asset_path(&asset_paths, metadata_path)?;
    assets
        .iter()
        .find(|asset| asset.path == cover_path)
        .map(|asset| asset.data_url.clone())
}

fn choose_cover_asset_path(asset_paths: &[String], metadata_path: Option<&str>) -> Option<String> {
    if let Some(path) = metadata_path
        .map(normalize_zip_path)
        .filter(|path| asset_paths.iter().any(|asset_path| asset_path == path))
    {
        return Some(path);
    }
    let mut image_paths = asset_paths
        .iter()
        .filter(|path| is_image_path(path))
        .cloned()
        .collect::<Vec<_>>();
    image_paths.sort();
    image_paths
        .iter()
        .find(|path| is_likely_cover_asset_path(path))
        .cloned()
        .or_else(|| image_paths.first().cloned())
}

fn is_likely_cover_asset_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    ["cover", "front", "title_page", "title-page", "titlepage"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn read_asset_data_url_from_archive(
    archive: &mut ZipArchive<File>,
    path: &str,
) -> Result<String, String> {
    let mut entry = archive
        .by_name(path)
        .map_err(|error| format!("could not read cover asset {path}: {error}"))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read cover asset {path}: {error}"))?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let encoded = general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn is_image_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
            )
        })
        .unwrap_or(false)
}

fn normalize_zip_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn order_chapter_paths_from_readme(chapter_paths: &mut [String], readme: &str) {
    let toc = readme_chapter_links(readme);
    if toc.is_empty() {
        chapter_paths.sort();
        return;
    }

    chapter_paths.sort_by(|left, right| {
        let left_index = toc.iter().position(|(_, path)| path == left);
        let right_index = toc.iter().position(|(_, path)| path == right);
        match (left_index, right_index) {
            (Some(left_index), Some(right_index)) => left_index.cmp(&right_index),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.cmp(right),
        }
    });
}

fn order_chapters_from_readme(chapters: &mut [BookChapter], readme: &str) {
    let toc = readme_chapter_links(readme);
    if toc.is_empty() {
        chapters.sort_by(|left, right| left.path.cmp(&right.path));
        return;
    }

    for chapter in chapters.iter_mut() {
        if let Some((title, _)) = toc.iter().find(|(_, path)| path == &chapter.path) {
            chapter.title = title.clone();
        }
    }

    chapters.sort_by(|left, right| {
        let left_index = toc.iter().position(|(_, path)| path == &left.path);
        let right_index = toc.iter().position(|(_, path)| path == &right.path);
        match (left_index, right_index) {
            (Some(left_index), Some(right_index)) => left_index.cmp(&right_index),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.path.cmp(&right.path),
        }
    });
}

fn readme_chapter_links(readme: &str) -> Vec<(String, String)> {
    readme
        .lines()
        .filter_map(|line| {
            let title_start = line.find('[')? + 1;
            let title_end = line[title_start..].find(']')? + title_start;
            let link_prefix = line[title_end..].find("](")? + title_end + 2;
            let link_end = line[link_prefix..].find(')')? + link_prefix;
            let title = line[title_start..title_end].trim();
            let path = normalize_zip_path(
                line[link_prefix..link_end]
                    .split('#')
                    .next()
                    .unwrap_or_default(),
            );
            if title.is_empty() || !path.starts_with("chapters/") || !path.ends_with(".md") {
                return None;
            }
            Some((title.to_string(), path))
        })
        .collect()
}

fn chapter_title(markdown: &str, path: &str) -> String {
    markdown
        .lines()
        .find_map(|line| {
            line.strip_prefix("# ")
                .or_else(|| line.strip_prefix("## "))
                .map(str::trim)
        })
        .filter(|line| !line.is_empty())
        .map(strip_inline_markdown)
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(path)
                .trim_start_matches(|character: char| {
                    character.is_ascii_digit() || character == '-' || character == '_'
                })
                .to_string()
        })
}

fn strip_inline_markdown(input: &str) -> String {
    input
        .replace(['*', '`', '#'], "")
        .replace("\\[", "[")
        .replace("\\]", "]")
}

fn confined_existing_file(path: &Path, root: &Path, label: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("could not resolve {label} directory: {error}"))?;
    let path = path
        .canonicalize()
        .map_err(|error| format!("could not resolve {label} path: {error}"))?;
    if !path.starts_with(&root) {
        return Err(format!(
            "{label} file must be inside {}",
            root.to_string_lossy()
        ));
    }
    if !path.is_file() {
        return Err(format!("{label} path is not a file: {}", path.display()));
    }
    Ok(path)
}

fn require_extension(path: &Path, extensions: &[&str], message: &str) -> Result<(), String> {
    if has_extension(path, extensions) {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extensions
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

fn modified_ms(path: &Path) -> u64 {
    let millis = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn now_ms() -> u64 {
    let millis = UNIX_EPOCH
        .elapsed()
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let window_builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("goosereader")
                    .inner_size(1120.0, 820.0)
                    .min_inner_size(760.0, 560.0);

            #[cfg(target_os = "macos")]
            let window_builder = window_builder
                .hidden_title(true)
                .title_bar_style(tauri::TitleBarStyle::Overlay);

            window_builder.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_paths,
            import_epub,
            list_books,
            list_themes,
            load_reading_progress,
            load_book_file,
            load_book_zip,
            load_theme_css,
            load_theme_file,
            save_reading_progress,
            set_book_completed,
            system_appearance,
            delete_book
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        delete_book_in_dir, list_books_in_dir, list_books_in_dir_with_progress, list_themes_in_dir,
        load_book_archive, load_book_file_in_dir, load_reading_progress_in_dir,
        load_theme_css_in_dir, order_chapters_from_readme, parse_bool_command_output,
        read_progress_store_from, save_reading_progress_in_dir, set_book_completed_in_dir,
        unique_import_destination, BookChapter,
    };
    use std::{fs::File, io::Write};
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn loads_epubmd_zip_payload() {
        let path =
            std::env::temp_dir().join(format!("goosereader-test-{}.zip", std::process::id()));
        write_test_book(&path);

        let payload = load_book_archive(&path).expect("load book");
        assert_eq!(payload.title, "Test Book");
        assert_eq!(payload.chapters.len(), 1);
        assert_eq!(payload.chapters[0].title, "One");
        assert_eq!(payload.assets[0].path, "assets/pic.png");
        assert!(payload.assets[0]
            .data_url
            .starts_with("data:image/png;base64,"));
        assert!(payload
            .cover_image
            .as_deref()
            .unwrap_or_default()
            .starts_with("data:image/png;base64,"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loads_zmd_payload() {
        let path =
            std::env::temp_dir().join(format!("goosereader-test-{}.zmd", std::process::id()));
        write_test_book(&path);

        let root = path.parent().unwrap();
        let payload = load_book_file_in_dir(&path, root).expect("load zmd book");

        assert_eq!(payload.title, "Test Book");
        assert_eq!(payload.chapters.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn readme_links_control_chapter_order_and_titles() {
        let readme =
            "- [Second in TOC](chapters/002-Two.md)\n- [First in TOC](chapters/001-One.md)\n";
        let mut chapters = vec![
            BookChapter {
                path: "chapters/001-One.md".into(),
                title: "One".into(),
                markdown: "# One".into(),
            },
            BookChapter {
                path: "chapters/002-Two.md".into(),
                title: "Two".into(),
                markdown: "# Two".into(),
            },
        ];

        order_chapters_from_readme(&mut chapters, readme);

        assert_eq!(chapters[0].path, "chapters/002-Two.md");
        assert_eq!(chapters[0].title, "Second in TOC");
        assert_eq!(chapters[1].path, "chapters/001-One.md");
        assert_eq!(chapters[1].title, "First in TOC");
    }

    #[test]
    fn lists_zmd_books_and_css_themes_from_app_dirs() {
        let root =
            std::env::temp_dir().join(format!("goosereader-library-test-{}", std::process::id()));
        let books_dir = root.join("books");
        let themes_dir = root.join("themes");
        std::fs::create_dir_all(&books_dir).expect("create books dir");
        std::fs::create_dir_all(&themes_dir).expect("create themes dir");
        write_test_book(&books_dir.join("book.zmd"));
        write_test_book(&books_dir.join("legacy.zip"));
        std::fs::write(
            themes_dir.join("typora-newsprint.css"),
            "#write { color: #111; }",
        )
        .expect("write theme");

        let books = list_books_in_dir(&books_dir).expect("list books");
        let themes = list_themes_in_dir(&themes_dir).expect("list themes");

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Test Book");
        assert_eq!(books[0].chapter_count, 1);
        assert!(books[0].cover_image.is_some());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "typora-newsprint");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn book_and_theme_loaders_reject_paths_outside_app_dirs() {
        let root =
            std::env::temp_dir().join(format!("goosereader-confine-test-{}", std::process::id()));
        let books_dir = root.join("books");
        let themes_dir = root.join("themes");
        let outside_dir = root.join("outside");
        std::fs::create_dir_all(&books_dir).expect("create books dir");
        std::fs::create_dir_all(&themes_dir).expect("create themes dir");
        std::fs::create_dir_all(&outside_dir).expect("create outside dir");
        let outside_book = outside_dir.join("outside.zmd");
        let outside_theme = outside_dir.join("outside.css");
        write_test_book(&outside_book);
        std::fs::write(&outside_theme, "#write { color: red; }").expect("write outside theme");

        let book_error = load_book_file_in_dir(&outside_book, &books_dir)
            .err()
            .expect("reject book");
        let theme_error = load_theme_css_in_dir(&outside_theme, &themes_dir)
            .err()
            .expect("reject theme");

        assert!(book_error.contains("book file must be inside"));
        assert!(theme_error.contains("theme file must be inside"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn import_destination_uses_zmd_and_avoids_overwrite() {
        let root =
            std::env::temp_dir().join(format!("goosereader-import-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create root");
        let source = root.join("My Book!.epub");
        std::fs::write(&source, b"not a real epub").expect("write source");
        std::fs::write(root.join("My-Book.zmd"), b"existing").expect("write existing");

        let destination = unique_import_destination(&source, &root).expect("destination");

        assert_eq!(destination.file_name().unwrap(), "My-Book-2.zmd");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn loads_theme_css_file() {
        let path =
            std::env::temp_dir().join(format!("goosereader-theme-test-{}.css", std::process::id()));
        std::fs::write(&path, "#write { font-family: serif; }").expect("write theme");

        let root = path.parent().unwrap();
        let theme = load_theme_css_in_dir(&path, root).expect("load theme");

        assert_eq!(theme.name, path.file_stem().unwrap().to_string_lossy());
        assert!(theme.css.contains("#write"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_macos_dark_mode_command_output() {
        assert_eq!(parse_bool_command_output(b"true\n"), Some(true));
        assert_eq!(parse_bool_command_output(b"false\n"), Some(false));
        assert_eq!(parse_bool_command_output(b"unexpected\n"), None);
    }

    #[test]
    fn saves_loads_and_deletes_book_progress() {
        let root =
            std::env::temp_dir().join(format!("goosereader-progress-test-{}", std::process::id()));
        let books_dir = root.join("books");
        let progress_path = root.join("reading-progress.json");
        std::fs::create_dir_all(&books_dir).expect("create books dir");
        let book_path = books_dir.join("book.zmd");
        write_test_book(&book_path);

        save_reading_progress_in_dir(
            &book_path,
            "chapters/001-One.md",
            Some(7),
            Some(420),
            &books_dir,
            &progress_path,
        )
        .expect("save progress");
        let progress = load_reading_progress_in_dir(&book_path, &books_dir, &progress_path)
            .expect("load progress")
            .expect("progress exists");
        assert_eq!(progress.chapter_path, "chapters/001-One.md");
        assert_eq!(progress.page_index, Some(7));
        assert_eq!(progress.scroll_top, Some(420));
        assert!(!progress.completed);

        let progress_store = read_progress_store_from(&progress_path).expect("read progress store");
        let books =
            list_books_in_dir_with_progress(&books_dir, &progress_store).expect("list books");
        assert_eq!(
            books[0].progress_chapter_path.as_deref(),
            Some("chapters/001-One.md")
        );
        assert_eq!(books[0].reading_progress_percent, Some(99));
        assert!(!books[0].completed);

        set_book_completed_in_dir(
            &book_path,
            true,
            Some("chapters/001-One.md"),
            &books_dir,
            &progress_path,
        )
        .expect("mark completed");
        let progress = load_reading_progress_in_dir(&book_path, &books_dir, &progress_path)
            .expect("load completed progress")
            .expect("progress exists");
        assert_eq!(progress.chapter_path, "chapters/001-One.md");
        assert_eq!(progress.page_index, Some(7));
        assert_eq!(progress.scroll_top, Some(420));
        assert!(progress.completed);
        let progress_store = read_progress_store_from(&progress_path).expect("read progress store");
        let books =
            list_books_in_dir_with_progress(&books_dir, &progress_store).expect("list books");
        assert_eq!(books[0].progress_chapter_path, None);
        assert_eq!(books[0].reading_progress_percent, None);
        assert!(books[0].completed);

        set_book_completed_in_dir(&book_path, false, None, &books_dir, &progress_path)
            .expect("mark not completed");
        let progress_store = read_progress_store_from(&progress_path).expect("read progress store");
        let books =
            list_books_in_dir_with_progress(&books_dir, &progress_store).expect("list books");
        assert_eq!(
            books[0].progress_chapter_path.as_deref(),
            Some("chapters/001-One.md")
        );
        assert_eq!(books[0].reading_progress_percent, Some(99));
        assert!(!books[0].completed);

        delete_book_in_dir(&book_path, &books_dir, &progress_path).expect("delete book");

        assert!(!book_path.exists());
        let store = read_progress_store_from(&progress_path).expect("read progress store");
        assert!(store.books.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_book_tolerates_corrupt_progress_store() {
        let root = std::env::temp_dir().join(format!(
            "goosereader-corrupt-progress-test-{}",
            std::process::id()
        ));
        let books_dir = root.join("books");
        let progress_path = root.join("reading-progress.json");
        std::fs::create_dir_all(&books_dir).expect("create books dir");
        let book_path = books_dir.join("book.zmd");
        write_test_book(&book_path);
        std::fs::write(&progress_path, "{not json").expect("write corrupt progress");

        delete_book_in_dir(&book_path, &books_dir, &progress_path).expect("delete book");

        assert!(!book_path.exists());
        let store = read_progress_store_from(&progress_path).expect("read repaired progress");
        assert!(store.books.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    fn write_test_book(path: &std::path::Path) {
        let file = File::create(path).expect("create test zip");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("README.md", options).unwrap();
        zip.write_all(b"# Test Book\n\n- [One](chapters/001-One.md)\n")
            .unwrap();
        zip.start_file("metadata.json", options).unwrap();
        zip.write_all(br#"{"cover_asset_path":"assets/pic.png"}"#)
            .unwrap();
        zip.start_file("style.css", options).unwrap();
        zip.write_all(b"body { color: #111827; }\n").unwrap();
        zip.start_file("chapters/001-One.md", options).unwrap();
        zip.write_all(b"# One\n\n![Pic](../assets/pic.png)\n")
            .unwrap();
        zip.start_file("assets/pic.png", options).unwrap();
        zip.write_all(&[0x89, 0x50, 0x4e, 0x47]).unwrap();
        zip.finish().unwrap();
    }
}
