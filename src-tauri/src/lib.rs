use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};
use zip::ZipArchive;

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

#[tauri::command]
fn load_book_zip(path: String) -> Result<BookPayload, String> {
    let file = File::open(&path).map_err(|error| format!("could not open zip: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("invalid zip archive: {error}"))?;

    let mut readme = String::new();
    let mut style_css = String::new();
    let mut chapters = Vec::new();
    let mut assets = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| format!("could not read zip entry: {error}"))?;
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
            entry.read_to_end(&mut bytes).map_err(|error| format!("could not read asset {entry_path}: {error}"))?;
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
        return Err("zip does not contain chapters/*.md files".into());
    }

    let title = readme
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Path::new(&path).file_stem().and_then(|stem| stem.to_str()).map(ToOwned::to_owned))
        .unwrap_or_else(|| "epubmd book".to_string());

    Ok(BookPayload {
        title,
        readme,
        style_css,
        chapters,
        assets,
    })
}

fn read_utf8_entry<R: Read>(reader: &mut R, path: &str) -> Result<String, String> {
    let mut text = String::new();
    reader.read_to_string(&mut text).map_err(|error| format!("{path} is not valid UTF-8: {error}"))?;
    Ok(text)
}

fn normalize_zip_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

fn order_chapters_from_readme(chapters: &mut Vec<BookChapter>, readme: &str) {
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
            let path = normalize_zip_path(line[link_prefix..link_end].split('#').next().unwrap_or_default());
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
        .find_map(|line| line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")).map(str::trim))
        .filter(|line| !line.is_empty())
        .map(strip_inline_markdown)
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(path)
                .trim_start_matches(|character: char| character.is_ascii_digit() || character == '-' || character == '_')
                .to_string()
        })
}

fn strip_inline_markdown(input: &str) -> String {
    input
        .replace(['*', '`', '#'], "")
        .replace("\\[", "[")
        .replace("\\]", "]")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![load_book_zip])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{load_book_zip, order_chapters_from_readme, BookChapter};
    use std::{fs::File, io::Write};
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn loads_epubmd_zip_payload() {
        let path = std::env::temp_dir().join(format!("epubmd-reader-test-{}.zip", std::process::id()));
        let file = File::create(&path).expect("create test zip");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("README.md", options).unwrap();
        zip.write_all(b"# Test Book\n\n- [One](chapters/001-One.md)\n").unwrap();
        zip.start_file("style.css", options).unwrap();
        zip.write_all(b"body { color: #111827; }\n").unwrap();
        zip.start_file("chapters/001-One.md", options).unwrap();
        zip.write_all(b"# One\n\n![Pic](../assets/pic.png)\n").unwrap();
        zip.start_file("assets/pic.png", options).unwrap();
        zip.write_all(&[0x89, 0x50, 0x4e, 0x47]).unwrap();
        zip.finish().unwrap();

        let payload = load_book_zip(path.to_string_lossy().to_string()).expect("load book");
        assert_eq!(payload.title, "Test Book");
        assert_eq!(payload.chapters.len(), 1);
        assert_eq!(payload.chapters[0].title, "One");
        assert_eq!(payload.assets[0].path, "assets/pic.png");
        assert!(payload.assets[0].data_url.starts_with("data:image/png;base64,"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn readme_links_control_chapter_order_and_titles() {
        let readme = "- [Second in TOC](chapters/002-Two.md)\n- [First in TOC](chapters/001-One.md)\n";
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
}
