use regex::Regex;
use roxmltree::{Document, Node, NodeType};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Debug)]
pub enum ConversionError {
    InvalidEpub(String),
    MalformedEpub(String),
    ProtectedEpub(String),
    FileSystem(String),
    Conversion(String),
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEpub(message) => write!(formatter, "Invalid EPUB: {message}"),
            Self::MalformedEpub(message) => write!(formatter, "Malformed EPUB: {message}"),
            Self::ProtectedEpub(message) => write!(formatter, "Protected EPUB: {message}"),
            Self::FileSystem(message) => write!(formatter, "File system error: {message}"),
            Self::Conversion(message) => write!(formatter, "Conversion error: {message}"),
        }
    }
}

impl std::error::Error for ConversionError {}

#[derive(Debug, Clone)]
struct ManifestItem {
    href: String,
    media_type: String,
    absolute_path: String,
}

#[derive(Debug, Clone)]
struct SpineItem {
    item: ManifestItem,
}

#[derive(Debug, Clone, Default)]
struct Metadata {
    title: String,
    creators: Vec<String>,
    language: Option<String>,
    publisher: Option<String>,
    date: Option<String>,
    identifier: Option<String>,
}

#[derive(Debug, Clone)]
struct EpubPackage {
    title: String,
    manifest: HashMap<String, ManifestItem>,
    spine: Vec<SpineItem>,
    metadata: Metadata,
}

#[derive(Debug, Clone)]
struct ChapterData {
    item: ManifestItem,
    data: Vec<u8>,
    file_name: String,
    title: Option<String>,
}

#[derive(Debug, Clone)]
struct AssetData {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
struct AssetMapper {
    href_to_markdown_path: HashMap<String, String>,
    copied_assets: Vec<AssetData>,
    used_names: HashSet<String>,
}

impl AssetMapper {
    fn copy_assets_from_manifest(
        &mut self,
        package: &EpubPackage,
        entries: &HashMap<String, Vec<u8>>,
    ) {
        let mut items: Vec<_> = package.manifest.values().collect();
        items.sort_by(|left, right| left.absolute_path.cmp(&right.absolute_path));
        for item in items {
            if !is_asset(item) {
                continue;
            }
            let Some(bytes) = entries.get(&item.absolute_path) else {
                continue;
            };
            let file_name = self.unique_file_name(&item.absolute_path);
            let markdown_path = format!("../assets/{file_name}");
            self.href_to_markdown_path
                .insert(item.absolute_path.clone(), markdown_path.clone());
            self.href_to_markdown_path
                .insert(item.href.clone(), markdown_path.clone());
            self.copied_assets.push(AssetData {
                path: format!("assets/{file_name}"),
                bytes: bytes.clone(),
            });
        }
    }

    fn copy_referenced_asset_if_present(
        &mut self,
        href: &str,
        content_base_directory: &str,
        entries: &HashMap<String, Vec<u8>>,
    ) -> Result<Option<String>, ConversionError> {
        let no_fragment = remove_fragment(href);
        let absolute = normalize_path(&join_path(content_base_directory, &no_fragment));
        if let Some(existing) = self.markdown_path(href, content_base_directory) {
            return Ok(Some(existing));
        }
        let Some(bytes) = entries.get(&absolute) else {
            return Ok(None);
        };
        let file_name = self.unique_file_name(&absolute);
        let markdown_path = format!("../assets/{file_name}");
        self.href_to_markdown_path
            .insert(absolute.clone(), markdown_path.clone());
        self.href_to_markdown_path
            .insert(no_fragment, markdown_path.clone());
        self.copied_assets.push(AssetData {
            path: format!("assets/{file_name}"),
            bytes: bytes.clone(),
        });
        Ok(Some(markdown_path))
    }

    fn markdown_path(&self, href: &str, content_base_directory: &str) -> Option<String> {
        let no_fragment = remove_fragment(href);
        let absolute = normalize_path(&join_path(content_base_directory, &no_fragment));
        self.href_to_markdown_path
            .get(&absolute)
            .or_else(|| self.href_to_markdown_path.get(&no_fragment))
            .cloned()
    }

    fn unique_file_name(&mut self, path: &str) -> String {
        let path = Path::new(path);
        let stem = path.with_extension("").to_string_lossy().to_string();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let re = Regex::new(r"[^A-Za-z0-9_-]+").expect("valid regex");
        let mut base = re.replace_all(&stem, "-").trim_matches('-').to_string();
        if base.is_empty() {
            base = "asset".to_string();
        }
        let mut candidate = if extension.is_empty() {
            base.clone()
        } else {
            format!("{base}.{extension}")
        };
        let mut index = 2;
        while self.used_names.contains(&candidate) {
            candidate = if extension.is_empty() {
                format!("{base}-{index}")
            } else {
                format!("{base}-{index}.{extension}")
            };
            index += 1;
        }
        self.used_names.insert(candidate.clone());
        candidate
    }
}

#[derive(Debug, Clone)]
struct ChapterLinkMap {
    epub_path_to_markdown: HashMap<String, String>,
}

impl ChapterLinkMap {
    fn markdown_path(&self, href: &str, current_epub_path: &str) -> Option<String> {
        let lower = href.to_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            return Some(href.to_string());
        }
        let target_no_fragment = remove_fragment(href);
        let fragment = fragment(href);
        if target_no_fragment.is_empty() {
            return fragment.map(|value| format!("#{value}"));
        }
        let base = parent_dir(current_epub_path);
        let normalized = normalize_path(&join_path(&base, &target_no_fragment));
        let markdown = self.epub_path_to_markdown.get(&normalized)?;
        Some(match fragment {
            Some(fragment) => format!("{markdown}#{fragment}"),
            None => markdown.clone(),
        })
    }
}

pub fn convert_epub_to_zip(
    epub_path: impl AsRef<Path>,
    output_zip_path: impl AsRef<Path>,
    overwrite: bool,
) -> Result<PathBuf, ConversionError> {
    let epub_path = epub_path.as_ref();
    let mut destination = output_zip_path.as_ref().to_path_buf();
    let supported_archive_extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip") || value.eq_ignore_ascii_case("zmd"))
        .unwrap_or(false);
    if !supported_archive_extension {
        destination.set_extension("zip");
    }
    if destination.exists() {
        if !overwrite {
            return Err(ConversionError::FileSystem(format!(
                "destination archive already exists: {}. Use --force to replace it",
                destination.display()
            )));
        }
        std::fs::remove_file(&destination).map_err(|error| {
            ConversionError::FileSystem(format!("could not remove destination zip: {error}"))
        })?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ConversionError::FileSystem(format!("could not create destination directory: {error}"))
        })?;
    }

    let entries = read_epub_entries(epub_path)?;
    let package = parse_package(&entries)?;

    let mut chapters = Vec::new();
    let mut chapter_output_by_epub_path = HashMap::new();
    for (offset, spine_item) in package.spine.iter().enumerate() {
        let data = entries
            .get(&spine_item.item.absolute_path)
            .ok_or_else(|| {
                ConversionError::MalformedEpub(format!(
                    "spine item missing content file: {}",
                    spine_item.item.absolute_path
                ))
            })?
            .clone();
        let title = chapter_title(&data);
        let file_name = chapter_file_name(offset + 1, title.as_deref(), &spine_item.item.href);
        chapter_output_by_epub_path
            .insert(spine_item.item.absolute_path.clone(), file_name.clone());
        chapters.push(ChapterData {
            item: spine_item.item.clone(),
            data,
            file_name,
            title,
        });
    }

    let mut asset_mapper = AssetMapper::default();
    asset_mapper.copy_assets_from_manifest(&package, &entries);
    for chapter in &chapters {
        ensure_referenced_images_are_available(
            &chapter.data,
            &chapter.item.absolute_path,
            &entries,
            &mut asset_mapper,
        )?;
    }

    let chapter_links = ChapterLinkMap {
        epub_path_to_markdown: chapter_output_by_epub_path,
    };
    let mut output_chapters = Vec::new();
    for chapter in &chapters {
        let markdown = convert_xhtml_to_markdown(
            &chapter.data,
            &chapter.item.absolute_path,
            &asset_mapper,
            &chapter_links,
        )?;
        output_chapters.push((
            chapter.file_name.clone(),
            chapter
                .title
                .clone()
                .unwrap_or_else(|| fallback_chapter_title(&chapter.file_name)),
            markdown,
        ));
    }

    let file = File::create(&destination)
        .map_err(|error| ConversionError::FileSystem(format!("could not create zip: {error}")))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory("chapters/", options)
        .map_err(zip_error("could not add chapters directory"))?;
    for (file_name, _, markdown) in &output_chapters {
        zip.start_file(format!("chapters/{file_name}"), options)
            .map_err(zip_error("could not add chapter"))?;
        zip.write_all(markdown.as_bytes()).map_err(|error| {
            ConversionError::FileSystem(format!("could not write chapter: {error}"))
        })?;
    }
    zip.start_file("README.md", options)
        .map_err(zip_error("could not add README.md"))?;
    zip.write_all(readme(&package, &output_chapters).as_bytes())
        .map_err(|error| {
            ConversionError::FileSystem(format!("could not write README.md: {error}"))
        })?;
    zip.start_file("style.css", options)
        .map_err(zip_error("could not add style.css"))?;
    zip.write_all(reader_style().as_bytes()).map_err(|error| {
        ConversionError::FileSystem(format!("could not write style.css: {error}"))
    })?;
    zip.add_directory("assets/", options)
        .map_err(zip_error("could not add assets directory"))?;
    for asset in &asset_mapper.copied_assets {
        zip.start_file(&asset.path, options)
            .map_err(zip_error("could not add asset"))?;
        zip.write_all(&asset.bytes).map_err(|error| {
            ConversionError::FileSystem(format!("could not write asset: {error}"))
        })?;
    }
    zip.finish().map_err(zip_error("could not finish zip"))?;
    Ok(destination)
}

fn zip_error(context: &'static str) -> impl FnOnce(zip::result::ZipError) -> ConversionError {
    move |error| ConversionError::FileSystem(format!("{context}: {error}"))
}

fn read_epub_entries(epub_path: &Path) -> Result<HashMap<String, Vec<u8>>, ConversionError> {
    let file = File::open(epub_path).map_err(|error| {
        ConversionError::InvalidEpub(format!(
            "file does not exist or cannot be opened: {} ({error})",
            epub_path.display()
        ))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        ConversionError::InvalidEpub(format!("not a readable zip-based EPUB: {error}"))
    })?;
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            ConversionError::InvalidEpub(format!("could not read zip entry: {error}"))
        })?;
        if entry.is_dir() {
            continue;
        }
        let path = normalize_path(&entry.name().replace('\\', "/"));
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| {
            ConversionError::ProtectedEpub(format!(
                "resource could not be read normally: {path} ({error})"
            ))
        })?;
        entries.insert(path, bytes);
    }
    if entries.contains_key("META-INF/encryption.xml") {
        return Err(ConversionError::ProtectedEpub(
            "META-INF/encryption.xml is present; DRM/protected content is unsupported".to_string(),
        ));
    }
    Ok(entries)
}

fn parse_package(entries: &HashMap<String, Vec<u8>>) -> Result<EpubPackage, ConversionError> {
    let container = entries.get("META-INF/container.xml").ok_or_else(|| {
        ConversionError::InvalidEpub("missing META-INF/container.xml".to_string())
    })?;
    let container_text = std::str::from_utf8(container).map_err(|_| {
        ConversionError::MalformedEpub("container.xml is not valid UTF-8".to_string())
    })?;
    let container_doc = Document::parse(container_text).map_err(|_| {
        ConversionError::MalformedEpub("container.xml could not be parsed".to_string())
    })?;
    let root_file_path = container_doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            ConversionError::MalformedEpub(
                "container.xml does not declare an OPF rootfile".to_string(),
            )
        })?;
    let root_file_path = normalize_path(root_file_path);
    let opf = entries.get(&root_file_path).ok_or_else(|| {
        ConversionError::MalformedEpub(format!("OPF file not found at {root_file_path}"))
    })?;
    let opf_text = std::str::from_utf8(opf).map_err(|_| {
        ConversionError::MalformedEpub(format!("OPF file is not valid UTF-8 at {root_file_path}"))
    })?;
    let opf_doc = Document::parse(opf_text).map_err(|_| {
        ConversionError::MalformedEpub(format!("OPF file could not be parsed at {root_file_path}"))
    })?;
    let base_directory = parent_dir(&root_file_path);
    let fallback_title = Path::new(&root_file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Book")
        .to_string();
    let metadata = parse_metadata(&opf_doc, &fallback_title);
    let title = metadata.title.clone();

    let mut manifest = HashMap::new();
    for item in opf_doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "item")
    {
        let Some(id) = item.attribute("id") else {
            continue;
        };
        let Some(href) = item.attribute("href") else {
            continue;
        };
        let media_type = item.attribute("media-type").unwrap_or("").to_string();
        let absolute_path = normalize_path(&join_path(&base_directory, href));
        manifest.insert(
            id.to_string(),
            ManifestItem {
                href: href.to_string(),
                media_type,
                absolute_path,
            },
        );
    }
    if manifest.is_empty() {
        return Err(ConversionError::MalformedEpub(
            "OPF manifest is empty".to_string(),
        ));
    }

    let mut spine = Vec::new();
    for itemref in opf_doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "itemref")
    {
        let Some(idref) = itemref.attribute("idref") else {
            continue;
        };
        let item = manifest
            .get(idref)
            .ok_or_else(|| {
                ConversionError::MalformedEpub(format!(
                    "spine references missing manifest item {idref}"
                ))
            })?
            .clone();
        spine.push(SpineItem { item });
    }
    if spine.is_empty() {
        return Err(ConversionError::MalformedEpub(
            "OPF spine is empty".to_string(),
        ));
    }

    Ok(EpubPackage {
        title,
        manifest,
        spine,
        metadata,
    })
}

fn parse_metadata(doc: &Document<'_>, fallback_title: &str) -> Metadata {
    let metadata_node = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "metadata");
    let scope = metadata_node.unwrap_or_else(|| doc.root_element());
    let title = first_descendant_text(scope, "title").unwrap_or_else(|| fallback_title.to_string());
    let creators = scope
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "creator")
        .filter_map(|node| collapsed_text(node))
        .collect();
    Metadata {
        title,
        creators,
        language: first_descendant_text(scope, "language"),
        publisher: first_descendant_text(scope, "publisher"),
        date: first_descendant_text(scope, "date"),
        identifier: first_descendant_text(scope, "identifier"),
    }
}

fn convert_xhtml_to_markdown(
    data: &[u8],
    current_epub_path: &str,
    asset_mapper: &AssetMapper,
    chapter_links: &ChapterLinkMap,
) -> Result<String, ConversionError> {
    let text = std::str::from_utf8(data).map_err(|_| {
        ConversionError::MalformedEpub(format!(
            "content file is not valid UTF-8: {current_epub_path}"
        ))
    })?;
    let content = strip_doctype(text);
    let doc = Document::parse(&content).map_err(|_| {
        ConversionError::MalformedEpub(format!(
            "content file could not be parsed: {current_epub_path}"
        ))
    })?;
    let root = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "body")
        .unwrap_or_else(|| doc.root_element());
    let context = RenderContext {
        current_epub_path,
        asset_mapper,
        chapter_links,
    };
    Ok(cleanup(&render_mixed_block_contents(root, &context)))
}

struct RenderContext<'a> {
    current_epub_path: &'a str,
    asset_mapper: &'a AssetMapper,
    chapter_links: &'a ChapterLinkMap,
}

fn render_block(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let name = node.tag_name().name();
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = name[1..].parse::<usize>().unwrap_or(1);
            format!(
                "{}{} {}",
                anchor_prefix(node),
                "#".repeat(level),
                trim_inline(&render_inline_children(node, context))
            )
        }
        "p" => format!(
            "{}{}",
            anchor_prefix(node),
            trim_inline(&render_inline_children(node, context))
        ),
        "ul" => render_list(node, false, context),
        "ol" => render_list(node, true, context),
        "blockquote" => {
            let body = render_mixed_block_contents(node, context);
            format!(
                "{}{}",
                anchor_prefix(node),
                body.lines()
                    .map(|line| format!("> {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
        "pre" => format!(
            "{}{}",
            anchor_prefix(node),
            fenced_code_block(&raw_text(node))
        ),
        "hr" => "---".to_string(),
        "table" => format!("{}{}", anchor_prefix(node), render_table(node, context)),
        "figure" => format!(
            "{}{}",
            anchor_prefix(node),
            node.children()
                .filter(|child| child.is_element())
                .map(|child| render_block(child, context))
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        ),
        "figcaption" => {
            format!("_{}_", trim_inline(&render_inline_children(node, context)))
        }
        "div" | "section" | "article" | "main" | "body" | "html" | "aside" | "nav" => format!(
            "{}{}",
            anchor_prefix(node),
            render_mixed_block_contents(node, context)
        ),
        "img" => format!("{}{}", anchor_prefix(node), render_image(node, context)),
        _ => trim_inline(&render_inline(node, context)),
    }
}

fn render_list(node: Node<'_, '_>, ordered: bool, context: &RenderContext<'_>) -> String {
    let mut lines = Vec::new();
    let mut index = 1;
    for child in node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "li")
    {
        let marker = if ordered {
            format!("{index}. ")
        } else {
            "- ".to_string()
        };
        let has_block_children = child.children().any(|grandchild| {
            grandchild.is_element()
                && is_block_element(grandchild.tag_name().name())
                && grandchild.tag_name().name() != "br"
        });
        let content = if has_block_children {
            let rendered = render_mixed_block_contents(child, context);
            if rendered.trim().is_empty() {
                render_inline_children(child, context)
            } else {
                rendered
            }
        } else {
            render_inline_children(child, context)
        };
        let normalized = content
            .lines()
            .enumerate()
            .map(|(offset, line)| {
                if offset == 0 {
                    format!("{marker}{line}")
                } else {
                    format!("{}{line}", " ".repeat(marker.len()))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(normalized);
        index += 1;
    }
    lines.join("\n")
}

fn render_mixed_block_contents(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let mut blocks = Vec::new();
    let mut inline_buffer = String::new();
    let flush = |buffer: &mut String, blocks: &mut Vec<String>| {
        let inline = trim_inline(buffer);
        if !inline.is_empty() {
            blocks.push(inline);
        }
        buffer.clear();
    };
    for content in node.children() {
        match content.node_type() {
            NodeType::Text => inline_buffer.push_str(content.text().unwrap_or("")),
            NodeType::Element => {
                if is_block_element(content.tag_name().name()) {
                    flush(&mut inline_buffer, &mut blocks);
                    let rendered = render_block(content, context).trim().to_string();
                    if !rendered.is_empty() {
                        blocks.push(rendered);
                    }
                } else {
                    inline_buffer.push_str(&render_inline(content, context));
                }
            }
            _ => {}
        }
    }
    flush(&mut inline_buffer, &mut blocks);
    blocks.join("\n\n")
}

fn render_inline_children(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let mut result = String::new();
    for content in node.children() {
        match content.node_type() {
            NodeType::Text => result.push_str(content.text().unwrap_or("")),
            NodeType::Element => {
                result.push_str(&render_inline(content, context));
                if matches!(content.tag_name().name(), "p" | "div" | "br") {
                    result.push(' ');
                }
            }
            _ => {}
        }
    }
    result
}

fn render_inline(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    match node.tag_name().name() {
        "strong" | "b" => format!(
            "**{}**",
            trim_inline(&render_inline_children(node, context))
        ),
        "em" | "i" => format!("*{}*", trim_inline(&render_inline_children(node, context))),
        "code" => format!(
            "`{}`",
            trim_inline(&render_inline_children(node, context)).replace('`', "\\`")
        ),
        "sup" => format!(
            "<sup>{}</sup>",
            trim_inline(&render_inline_children(node, context))
        ),
        "sub" => format!(
            "<sub>{}</sub>",
            trim_inline(&render_inline_children(node, context))
        ),
        "a" => {
            let text = trim_inline(&render_inline_children(node, context));
            let Some(href) = node.attribute("href").filter(|href| !href.is_empty()) else {
                return text;
            };
            let resolved = context
                .chapter_links
                .markdown_path(href, context.current_epub_path)
                .unwrap_or_else(|| href.to_string());
            let label = if text.is_empty() {
                resolved.clone()
            } else {
                text
            };
            format!("[{label}]({resolved})")
        }
        "img" => render_image(node, context),
        "br" => "\n".to_string(),
        "li" | "ul" | "ol" => render_block(node, context),
        _ => render_inline_children(node, context),
    }
}

fn render_table(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let rows: Vec<Vec<String>> = node
        .descendants()
        .filter(|descendant| descendant.is_element() && descendant.tag_name().name() == "tr")
        .map(|row| {
            row.children()
                .filter(|cell| cell.is_element() && matches!(cell.tag_name().name(), "th" | "td"))
                .map(|cell| trim_inline(&render_inline_children(cell, context)).replace('|', "\\|"))
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect();
    let Some(first) = rows.first() else {
        return trim_inline(&render_inline_children(node, context));
    };
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(first.len());
    let pad = |row: &[String]| -> Vec<String> {
        let mut padded = row.iter().take(column_count).cloned().collect::<Vec<_>>();
        padded.resize(column_count, String::new());
        padded
    };
    let mut lines = Vec::new();
    let header = pad(first);
    lines.push(format!("| {} |", header.join(" | ")));
    lines.push(format!("| {} |", vec!["---"; column_count].join(" | ")));
    for row in rows.iter().skip(1) {
        lines.push(format!("| {} |", pad(row).join(" | ")));
    }
    lines.join("\n")
}

fn render_image(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let Some(src) = node.attribute("src").filter(|src| !src.is_empty()) else {
        return String::new();
    };
    let alt = node.attribute("alt").unwrap_or("");
    let resolved = context
        .asset_mapper
        .markdown_path(src, &parent_dir(context.current_epub_path))
        .unwrap_or_else(|| src.to_string());
    format!("![{alt}]({resolved})")
}

fn ensure_referenced_images_are_available(
    data: &[u8],
    current_epub_path: &str,
    entries: &HashMap<String, Vec<u8>>,
    asset_mapper: &mut AssetMapper,
) -> Result<(), ConversionError> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Ok(());
    };
    let content = strip_doctype(text);
    let Ok(doc) = Document::parse(&content) else {
        return Ok(());
    };
    let content_base_directory = parent_dir(current_epub_path);
    for image in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "img")
    {
        let Some(src) = image.attribute("src").filter(|src| !src.is_empty()) else {
            continue;
        };
        let lower = src.to_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("data:")
        {
            continue;
        }
        if asset_mapper
            .copy_referenced_asset_if_present(src, &content_base_directory, entries)?
            .is_none()
        {
            return Err(ConversionError::MalformedEpub(format!(
                "referenced image asset is missing or not declared: {src}"
            )));
        }
    }
    Ok(())
}

fn chapter_title(data: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(data).ok()?;
    let content = strip_doctype(text);
    let doc = Document::parse(&content).ok()?;
    for heading in ["h1", "h2", "h3"] {
        if let Some(title) = doc
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == heading)
            .and_then(collapsed_text)
        {
            return Some(title);
        }
    }
    doc.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "title")
        .and_then(collapsed_text)
}

fn readme(package: &EpubPackage, chapters: &[(String, String, String)]) -> String {
    let mut readme = format!("# {}\n\n", package.title);
    let mut metadata_lines = Vec::new();
    if !package.metadata.creators.is_empty() {
        metadata_lines.push(format!(
            "- Author: {}",
            package.metadata.creators.join(", ")
        ));
    }
    if let Some(value) = &package.metadata.publisher {
        metadata_lines.push(format!("- Publisher: {value}"));
    }
    if let Some(value) = &package.metadata.date {
        metadata_lines.push(format!("- Date: {value}"));
    }
    if let Some(value) = &package.metadata.language {
        metadata_lines.push(format!("- Language: {value}"));
    }
    if let Some(value) = &package.metadata.identifier {
        metadata_lines.push(format!("- Identifier: {value}"));
    }
    if !metadata_lines.is_empty() {
        readme.push_str("## Metadata\n\n");
        readme.push_str(&metadata_lines.join("\n"));
        readme.push_str("\n\n");
    }
    readme.push_str("## Contents\n\n");
    for (file_name, title, _) in chapters {
        readme.push_str(&format!("- [{title}](chapters/{file_name})\n"));
    }
    readme.push_str("\n## Reading style\n\nIf your Markdown viewer supports custom stylesheets, use `style.css` for a higher-contrast book-like reading view.\n");
    readme
}

fn reader_style() -> &'static str {
    r#":root {
  color-scheme: light dark;
}

body {
  max-width: 78ch;
  margin: 3rem auto;
  padding: 0 1.5rem;
  color: #1f2937;
  background: #ffffff;
  font: 17px/1.65 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

h1, h2, h3, h4, h5, h6 {
  color: #111827;
  line-height: 1.25;
  margin-top: 2rem;
}

a {
  color: #0969da;
}

blockquote {
  color: #374151;
  border-left: 4px solid #d1d5db;
  margin-left: 0;
  padding-left: 1rem;
}

pre, code {
  color: #111827;
  background: #f6f8fa;
}

img {
  max-width: 100%;
  height: auto;
}

@media (prefers-color-scheme: dark) {
  body {
    color: #e5e7eb;
    background: #0f172a;
  }

  h1, h2, h3, h4, h5, h6,
  pre, code {
    color: #f9fafb;
  }

  a {
    color: #8ab4f8;
  }

  blockquote {
    color: #d1d5db;
    border-left-color: #4b5563;
  }

  pre, code {
    background: #111827;
  }
}
"#
}

fn strip_doctype(input: &str) -> String {
    Regex::new(r"(?is)<!DOCTYPE[^>]*>")
        .expect("valid regex")
        .replace(input, "")
        .to_string()
}

fn is_asset(item: &ManifestItem) -> bool {
    if item.media_type.to_lowercase().starts_with("image/") {
        return true;
    }
    Path::new(&item.href)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| {
            matches!(
                extension.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
            )
        })
        .unwrap_or(false)
}

fn first_descendant_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == name)
        .and_then(collapsed_text)
}

fn collapsed_text(node: Node<'_, '_>) -> Option<String> {
    let text = node
        .descendants()
        .filter(|descendant| descendant.node_type() == NodeType::Text)
        .filter_map(|descendant| descendant.text())
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed = Regex::new(r"\s+")
        .expect("valid regex")
        .replace_all(&text, " ")
        .trim()
        .to_string();
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

fn raw_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|descendant| descendant.node_type() == NodeType::Text)
        .filter_map(|descendant| descendant.text())
        .collect::<String>()
}

fn fenced_code_block(code: &str) -> String {
    let fence = if code.contains("```") { "~~~~" } else { "```" };
    format!("{fence}\n{}\n{fence}", code.trim())
}

fn anchor_prefix(node: Node<'_, '_>) -> String {
    node.attribute("id")
        .or_else(|| node.attribute("name"))
        .filter(|value| !value.trim().is_empty())
        .map(|id| format!("<a id=\"{id}\"></a>\n"))
        .unwrap_or_default()
}

fn cleanup(markdown: &str) -> String {
    let space_re = Regex::new(r"[ \t]+").expect("valid regex");
    let newline_re = Regex::new(r"\n{3,}").expect("valid regex");
    let value = space_re.replace_all(markdown, " ");
    let value = newline_re.replace_all(&value, "\n\n");
    let value = remove_duplicate_leading_title_blocks(&value);
    format!("{}\n", value.trim())
}

fn remove_duplicate_leading_title_blocks(markdown: &str) -> String {
    let mut blocks: Vec<String> = markdown.split("\n\n").map(ToOwned::to_owned).collect();
    if blocks.len() < 2 {
        return markdown.to_string();
    }
    let mut index = 1;
    while index < blocks.len().min(4) {
        let previous = title_comparable_text(&blocks[index - 1]);
        let current = title_comparable_text(&blocks[index]);
        if previous.is_empty() || previous != current {
            index += 1;
            continue;
        }
        let previous_is_heading = is_heading_block(&blocks[index - 1]);
        let current_is_heading = is_heading_block(&blocks[index]);
        let looks_like_leading_plain_title =
            index == 1 && previous.chars().count() <= 100 && current.chars().count() <= 100;
        if !(previous_is_heading || current_is_heading || looks_like_leading_plain_title) {
            index += 1;
            continue;
        }
        if current_is_heading || !previous_is_heading {
            blocks.remove(index - 1);
        } else {
            blocks.remove(index);
        }
    }
    blocks.join("\n\n")
}

fn is_heading_block(block: &str) -> bool {
    let heading_re = Regex::new(r"^#{1,6}\s+").expect("valid regex");
    block.lines().any(|line| heading_re.is_match(line.trim()))
}

fn title_comparable_text(block: &str) -> String {
    let anchor_re = Regex::new(r#"^<a\s+id="[^"]+"></a>$"#).expect("valid regex");
    let heading_re = Regex::new(r"^#{1,6}\s+").expect("valid regex");
    let tag_re = Regex::new(r"<[^>]+>").expect("valid regex");
    let emphasis_re = Regex::new(r"[*_`]+").expect("valid regex");
    let link_re = Regex::new(r"\[([^\]]+)\]\([^)]+\)").expect("valid regex");
    let whitespace_re = Regex::new(r"\s+").expect("valid regex");
    let lines = block
        .lines()
        .filter(|line| !anchor_re.is_match(line.trim()))
        .collect::<Vec<_>>()
        .join(" ");
    let value = heading_re.replace(&lines, "");
    let value = tag_re.replace_all(&value, "");
    let value = emphasis_re.replace_all(&value, "");
    let value = link_re.replace_all(&value, "$1");
    whitespace_re.replace_all(&value, " ").trim().to_string()
}

fn chapter_file_name(index: usize, title: Option<&str>, fallback: &str) -> String {
    let fallback_stem = Path::new(fallback)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("chapter");
    let base = sanitize_markdown_path_segment(
        title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(fallback_stem),
    );
    format!("{index:03}-{base}.md")
}

fn sanitize_markdown_path_segment(input: &str) -> String {
    let control_re = Regex::new(r#"[\p{C}/\\:?%*|"<>#\[\]()]"#).expect("valid regex");
    let whitespace_re = Regex::new(r"\s+").expect("valid regex");
    let dash_re = Regex::new(r"-+").expect("valid regex");
    let value = control_re.replace_all(input, "-");
    let value = whitespace_re.replace_all(&value, "-");
    let value = dash_re.replace_all(&value, "-");
    let value = value
        .trim_matches(|character: char| {
            character == ' ' || character == '.' || character == '-' || character == '_'
        })
        .to_string();
    if value.is_empty() {
        "chapter".to_string()
    } else {
        value
    }
}

fn fallback_chapter_title(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
        .trim_start_matches(|character: char| {
            character.is_ascii_digit() || character == '-' || character == '_'
        })
        .to_string()
}

fn trim_inline(input: &str) -> String {
    Regex::new(r"\s+")
        .expect("valid regex")
        .replace_all(input, " ")
        .trim()
        .to_string()
}

fn is_block_element(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "body"
            | "caption"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "html"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}

fn join_path(base: &str, path: &str) -> String {
    if base.is_empty() {
        path.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), path)
    }
}

fn normalize_path(path: &str) -> String {
    let mut normalized = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                normalized.pop();
            }
            value => normalized.push(value),
        }
    }
    normalized.join("/")
}

fn parent_dir(path: &str) -> String {
    let normalized = normalize_path(path);
    Path::new(&normalized)
        .parent()
        .and_then(|value| value.to_str())
        .filter(|value| *value != ".")
        .unwrap_or("")
        .replace('\\', "/")
}

fn remove_fragment(href: &str) -> String {
    href.split('#').next().unwrap_or(href).to_string()
}

fn fragment(href: &str) -> Option<String> {
    let (_, fragment) = href.split_once('#')?;
    Some(fragment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_parent_directories() {
        assert_eq!(
            normalize_path("EPUB/text/../media/resources/title_page.png"),
            "EPUB/media/resources/title_page.png"
        );
    }

    #[test]
    fn chapter_file_names_are_markdown_link_safe() {
        assert_eq!(
            chapter_file_name(4, Some("Multi-Paradigm Programming"), "body.xhtml"),
            "004-Multi-Paradigm-Programming.md"
        );
        assert_eq!(
            chapter_file_name(7, Some("第零七章 • 计算"), "chapter7.xhtml"),
            "007-第零七章-•-计算.md"
        );
    }

    #[test]
    fn removes_duplicate_leading_titles() {
        let markdown = "# 第零七章 • 计算\n\n第零七章 • 计算\n\n正文开始。";
        let cleaned = cleanup(markdown);
        assert_eq!(cleaned.matches("第零七章 • 计算").count(), 1);
    }

    #[test]
    fn preserves_zmd_destination_extension() {
        let epub = std::env::temp_dir().join(format!(
            "epubmd-zmd-extension-source-{}.epub",
            std::process::id()
        ));
        let destination = std::env::temp_dir().join(format!(
            "epubmd-zmd-extension-test-{}.zmd",
            std::process::id()
        ));
        write_minimal_epub(&epub);

        let written = convert_epub_to_zip(&epub, &destination, true).expect("convert epub");

        assert_eq!(written, destination);
        assert!(destination.exists());
        assert!(!destination.with_extension("zip").exists());

        let _ = std::fs::remove_file(epub);
        let _ = std::fs::remove_file(destination);
    }

    fn write_minimal_epub(path: &Path) {
        let file = File::create(path).expect("create epub");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();
        zip.start_file("OPS/package.opf", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>ZMD Test</dc:title>
  </metadata>
  <manifest>
    <item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chapter"/>
  </spine>
</package>"#,
        )
        .unwrap();
        zip.start_file("OPS/chapter.xhtml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter One</title></head><body><h1>Chapter One</h1><p>Hello.</p></body></html>"#,
        )
        .unwrap();
        zip.finish().unwrap();
    }
}
