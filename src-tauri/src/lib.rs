use base64::{engine::general_purpose, Engine as _};
use epubmd_core::convert_epub_to_zip;
use serde::Serialize;
use std::{
    env,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
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
    modified_ms: u64,
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

#[derive(Serialize)]
struct ThemePayload {
    name: String,
    css: String,
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
        .map_err(|error| format!("epubmd: {error}"))?;
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

    if chapters.is_empty() {
        return Err("book archive does not contain chapters/*.md files".into());
    }

    let title = title_from_readme(&readme)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "epubmd book".to_string());

    Ok(BookPayload {
        title,
        readme,
        style_css,
        chapters,
        assets,
    })
}

fn list_books_in_dir(books_dir: &Path) -> Result<Vec<LibraryBook>, String> {
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
            Ok(book) => books.push(book),
            Err(_) => books.push(fallback_library_book(&path)),
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
    let mut chapter_count = 0;

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
        } else if entry_path.starts_with("chapters/") && entry_path.ends_with(".md") {
            chapter_count += 1;
        }
    }

    let mut book = fallback_library_book(path);
    book.title = title_from_readme(&readme).unwrap_or(book.title);
    book.chapter_count = chapter_count;
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
        modified_ms: modified_ms(path),
    }
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
    if let Some(path) = env::var_os("EPUBMD_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| "could not locate home directory for ~/.config/epubmd".to_string())?;
    Ok(PathBuf::from(home).join(".config").join("epubmd"))
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

fn normalize_zip_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
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

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_paths,
            import_epub,
            list_books,
            list_themes,
            load_book_file,
            load_book_zip,
            load_theme_css,
            load_theme_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        list_books_in_dir, list_themes_in_dir, load_book_archive, load_book_file_in_dir,
        load_theme_css_in_dir, order_chapters_from_readme, unique_import_destination, BookChapter,
    };
    use std::{fs::File, io::Write};
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn loads_epubmd_zip_payload() {
        let path =
            std::env::temp_dir().join(format!("epubmd-reader-test-{}.zip", std::process::id()));
        write_test_book(&path);

        let payload = load_book_archive(&path).expect("load book");
        assert_eq!(payload.title, "Test Book");
        assert_eq!(payload.chapters.len(), 1);
        assert_eq!(payload.chapters[0].title, "One");
        assert_eq!(payload.assets[0].path, "assets/pic.png");
        assert!(payload.assets[0]
            .data_url
            .starts_with("data:image/png;base64,"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loads_zmd_payload() {
        let path =
            std::env::temp_dir().join(format!("epubmd-reader-test-{}.zmd", std::process::id()));
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
        let root = std::env::temp_dir().join(format!("epubmd-library-test-{}", std::process::id()));
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
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "typora-newsprint");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn book_and_theme_loaders_reject_paths_outside_app_dirs() {
        let root = std::env::temp_dir().join(format!("epubmd-confine-test-{}", std::process::id()));
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
        let root = std::env::temp_dir().join(format!("epubmd-import-test-{}", std::process::id()));
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
            std::env::temp_dir().join(format!("epubmd-theme-test-{}.css", std::process::id()));
        std::fs::write(&path, "#write { font-family: serif; }").expect("write theme");

        let root = path.parent().unwrap();
        let theme = load_theme_css_in_dir(&path, root).expect("load theme");

        assert_eq!(theme.name, path.file_stem().unwrap().to_string_lossy());
        assert!(theme.css.contains("#write"));

        let _ = std::fs::remove_file(path);
    }

    fn write_test_book(path: &std::path::Path) {
        let file = File::create(path).expect("create test zip");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("README.md", options).unwrap();
        zip.write_all(b"# Test Book\n\n- [One](chapters/001-One.md)\n")
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
