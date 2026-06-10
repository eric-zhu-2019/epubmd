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
    properties: String,
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
    nav_points: Vec<NavPoint>,
    cover_image_path: Option<String>,
}

#[derive(Debug, Clone)]
struct ChapterData {
    item: ManifestItem,
    data: Vec<u8>,
    file_name: String,
    title: Option<String>,
}

#[derive(Debug, Clone)]
struct NavPoint {
    title: String,
    epub_path: String,
    fragment: Option<String>,
    depth: usize,
}

#[derive(Debug, Clone)]
struct NavTarget {
    spine_index: usize,
    fragment: Option<String>,
}

#[derive(Debug, Clone)]
struct LogicalChapterSegment {
    title: String,
    file_name: String,
    nav_index: usize,
    start: NavTarget,
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

    fn copy_asset_at_path_if_present(
        &mut self,
        absolute_path: &str,
        entries: &HashMap<String, Vec<u8>>,
    ) -> Option<String> {
        if let Some(existing) = self.archive_asset_path_for_epub_path(absolute_path) {
            return Some(existing);
        }
        let bytes = entries.get(absolute_path)?;
        let file_name = self.unique_file_name(absolute_path);
        let markdown_path = format!("../assets/{file_name}");
        self.href_to_markdown_path
            .insert(absolute_path.to_string(), markdown_path.clone());
        self.copied_assets.push(AssetData {
            path: format!("assets/{file_name}"),
            bytes: bytes.clone(),
        });
        Some(markdown_path.trim_start_matches("../").to_string())
    }

    fn markdown_path(&self, href: &str, content_base_directory: &str) -> Option<String> {
        let no_fragment = remove_fragment(href);
        let absolute = normalize_path(&join_path(content_base_directory, &no_fragment));
        self.href_to_markdown_path
            .get(&absolute)
            .or_else(|| self.href_to_markdown_path.get(&no_fragment))
            .cloned()
    }

    fn archive_asset_path_for_epub_path(&self, epub_path: &str) -> Option<String> {
        self.href_to_markdown_path
            .get(epub_path)
            .map(|path| path.trim_start_matches("../").to_string())
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
        if let Some(fragment) = fragment.as_deref() {
            let exact = format!("{normalized}#{fragment}");
            if let Some(markdown) = self.epub_path_to_markdown.get(&exact) {
                return Some(markdown.clone());
            }
        }
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
        chapters.push(ChapterData {
            item: spine_item.item.clone(),
            data,
            file_name,
            title,
        });
    }
    let logical_segments = logical_chapter_segments(&chapters, &package.nav_points);

    let mut asset_mapper = AssetMapper::default();
    asset_mapper.copy_assets_from_manifest(&package, &entries);
    let cover_asset_path = package.cover_image_path.as_ref().and_then(|epub_path| {
        asset_mapper
            .archive_asset_path_for_epub_path(epub_path)
            .or_else(|| asset_mapper.copy_asset_at_path_if_present(epub_path, &entries))
    });
    for chapter in &chapters {
        ensure_referenced_images_are_available(
            &chapter.data,
            &chapter.item.absolute_path,
            &entries,
            &mut asset_mapper,
        )?;
    }

    let chapter_links = ChapterLinkMap {
        epub_path_to_markdown: chapter_output_map(
            &chapters,
            &package.nav_points,
            &logical_segments,
        ),
    };
    let mut converted_spines = Vec::new();
    for chapter in &chapters {
        let markdown = convert_xhtml_to_markdown(
            &chapter.data,
            &chapter.item.absolute_path,
            &asset_mapper,
            &chapter_links,
        )?;
        let markdown = promote_navigation_targets_to_headings(
            markdown,
            &chapter.item.absolute_path,
            &package.nav_points,
        );
        converted_spines.push(markdown);
    }
    let output_chapters = output_chapters_from_spines(
        &chapters,
        &converted_spines,
        &logical_segments,
        &package.nav_points,
        &chapter_links,
    );

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
    zip.start_file("metadata.json", options)
        .map_err(zip_error("could not add metadata.json"))?;
    zip.write_all(metadata_json(&package, cover_asset_path.as_deref()).as_bytes())
        .map_err(|error| {
            ConversionError::FileSystem(format!("could not write metadata.json: {error}"))
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
                properties: item.attribute("properties").unwrap_or("").to_lowercase(),
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

    let nav_points = parse_navigation_points(entries, &manifest, &opf_doc, &base_directory);
    let cover_image_path = find_cover_image_path(entries, &manifest, &opf_doc, &base_directory);

    Ok(EpubPackage {
        title,
        manifest,
        spine,
        metadata,
        nav_points,
        cover_image_path,
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

fn parse_navigation_points(
    entries: &HashMap<String, Vec<u8>>,
    manifest: &HashMap<String, ManifestItem>,
    opf_doc: &Document<'_>,
    opf_base_directory: &str,
) -> Vec<NavPoint> {
    let mut nav_items = Vec::new();
    let mut explicit_toc_items = Vec::new();
    let mut fallback_ncx_items = Vec::new();
    if let Some(toc_id) = opf_doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "spine")
        .and_then(|node| node.attribute("toc"))
    {
        if let Some(item) = manifest.get(toc_id) {
            explicit_toc_items.push(item.clone());
        }
    }
    for item in manifest.values() {
        let media_type = item.media_type.to_lowercase();
        let properties = manifest_item_properties(item, opf_doc);
        let is_ncx = media_type.contains("ncx") || item.href.to_lowercase().ends_with(".ncx");
        let is_nav = properties
            .split_whitespace()
            .any(|property| property == "nav");
        if is_nav {
            nav_items.push(item.clone());
        } else if is_ncx
            && !explicit_toc_items
                .iter()
                .any(|existing| existing.absolute_path == item.absolute_path)
        {
            fallback_ncx_items.push(item.clone());
        }
    }

    for items in [
        &nav_items[..],
        &explicit_toc_items[..],
        &fallback_ncx_items[..],
    ] {
        let points = navigation_points_from_items(entries, items);
        if !points.is_empty() {
            return points;
        }
    }

    parse_inline_navigation_hrefs(opf_doc, opf_base_directory)
}

fn find_cover_image_path(
    entries: &HashMap<String, Vec<u8>>,
    manifest: &HashMap<String, ManifestItem>,
    opf_doc: &Document<'_>,
    opf_base_directory: &str,
) -> Option<String> {
    if let Some(item) = manifest.values().find(|item| {
        item.properties
            .split_whitespace()
            .any(|property| property == "cover-image")
            && is_asset(item)
    }) {
        return Some(item.absolute_path.clone());
    }

    if let Some(cover_id) = opf_doc
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name() == "meta"
                && node
                    .attribute("name")
                    .is_some_and(|name| name.eq_ignore_ascii_case("cover"))
        })
        .and_then(|node| node.attribute("content"))
    {
        if let Some(item) = manifest.get(cover_id) {
            if is_asset(item) {
                return Some(item.absolute_path.clone());
            }
            if is_xhtml_item(item) {
                return first_image_in_xhtml(entries, &item.absolute_path);
            }
        }
    }

    for reference in opf_doc.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "reference"
            && node
                .attribute("type")
                .is_some_and(|value| value.eq_ignore_ascii_case("cover"))
    }) {
        let Some(href) = reference.attribute("href") else {
            continue;
        };
        let cover_page_path =
            normalize_path(&join_path(opf_base_directory, &remove_fragment(href)));
        if let Some(image_path) = first_image_in_xhtml(entries, &cover_page_path) {
            return Some(image_path);
        }
    }

    None
}

fn first_image_in_xhtml(entries: &HashMap<String, Vec<u8>>, xhtml_path: &str) -> Option<String> {
    let data = entries.get(xhtml_path)?;
    let text = std::str::from_utf8(data).ok()?;
    let content = strip_doctype(text);
    let doc = Document::parse(&content).ok()?;
    let src = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "img")
        .and_then(|node| node.attribute("src"))
        .filter(|src| !src.trim().is_empty())?;
    Some(normalize_path(&join_path(
        &parent_dir(xhtml_path),
        &remove_fragment(src),
    )))
}

fn is_xhtml_item(item: &ManifestItem) -> bool {
    let media_type = item.media_type.to_lowercase();
    media_type.contains("xhtml") || item.href.to_lowercase().ends_with(".xhtml")
}

fn navigation_points_from_items(
    entries: &HashMap<String, Vec<u8>>,
    navigation_items: &[ManifestItem],
) -> Vec<NavPoint> {
    let mut points = Vec::new();
    let mut seen_targets = HashSet::new();
    for item in navigation_items {
        let Some(bytes) = entries.get(&item.absolute_path) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let item_base = parent_dir(&item.absolute_path);
        let parsed = if item.media_type.to_lowercase().contains("ncx")
            || item.href.to_lowercase().ends_with(".ncx")
        {
            parse_ncx_points(text, &item_base)
        } else {
            parse_xhtml_nav_points(text, &item_base)
        };
        for point in parsed {
            let key = (point.epub_path.clone(), point.fragment.clone());
            if seen_targets.insert(key) {
                points.push(point);
            }
        }
    }
    points
}

fn manifest_item_properties(item: &ManifestItem, _opf_doc: &Document<'_>) -> String {
    item.properties.clone()
}

fn parse_ncx_points(text: &str, base_directory: &str) -> Vec<NavPoint> {
    let Ok(doc) = Document::parse(text) else {
        return Vec::new();
    };
    let Some(nav_map) = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "navMap")
    else {
        return Vec::new();
    };
    let mut points = Vec::new();
    for node in nav_map
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "navPoint")
    {
        collect_ncx_point(node, base_directory, 1, &mut points);
    }
    points
}

fn collect_ncx_point(
    node: Node<'_, '_>,
    base_directory: &str,
    depth: usize,
    points: &mut Vec<NavPoint>,
) {
    let title = node
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "navLabel")
        .and_then(|label| {
            label
                .descendants()
                .find(|descendant| {
                    descendant.is_element() && descendant.tag_name().name() == "text"
                })
                .and_then(collapsed_text)
                .or_else(|| collapsed_text(label))
        });
    let source = node
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "content")
        .and_then(|content| content.attribute("src"));
    if let (Some(title), Some(source)) = (title, source) {
        points.push(nav_point_from_href(title, source, base_directory, depth));
    }
    for child in node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "navPoint")
    {
        collect_ncx_point(child, base_directory, depth + 1, points);
    }
}

fn parse_xhtml_nav_points(text: &str, base_directory: &str) -> Vec<NavPoint> {
    let content = strip_doctype(text);
    let Ok(doc) = Document::parse(&content) else {
        return Vec::new();
    };
    let nav = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "nav" && is_toc_nav(*node))
        .or_else(|| {
            doc.descendants()
                .find(|node| node.is_element() && node.tag_name().name() == "nav")
        })
        .unwrap_or_else(|| doc.root_element());
    let mut points = Vec::new();
    for list in nav
        .children()
        .filter(|child| child.is_element() && matches!(child.tag_name().name(), "ol" | "ul"))
    {
        collect_xhtml_nav_list(list, base_directory, 1, &mut points);
    }
    points
}

fn is_toc_nav(node: Node<'_, '_>) -> bool {
    node.attributes().any(|attribute| {
        let name = attribute.name();
        matches!(name, "type" | "epub:type")
            && attribute
                .value()
                .split_whitespace()
                .any(|value| value.eq_ignore_ascii_case("toc"))
    })
}

fn collect_xhtml_nav_list(
    list: Node<'_, '_>,
    base_directory: &str,
    depth: usize,
    points: &mut Vec<NavPoint>,
) {
    for item in list
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "li")
    {
        if let Some(link) = item
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == "a")
        {
            if let Some(href) = link.attribute("href") {
                if let Some(title) = collapsed_text(link) {
                    points.push(nav_point_from_href(title, href, base_directory, depth));
                }
            }
        }
        for child_list in item
            .children()
            .filter(|child| child.is_element() && matches!(child.tag_name().name(), "ol" | "ul"))
        {
            collect_xhtml_nav_list(child_list, base_directory, depth + 1, points);
        }
    }
}

fn parse_inline_navigation_hrefs(doc: &Document<'_>, base_directory: &str) -> Vec<NavPoint> {
    doc.descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "a")
        .filter_map(|link| {
            let href = link.attribute("href")?;
            let title = collapsed_text(link)?;
            Some(nav_point_from_href(title, href, base_directory, 1))
        })
        .collect()
}

fn nav_point_from_href(title: String, href: &str, base_directory: &str, depth: usize) -> NavPoint {
    let no_fragment = remove_fragment(href);
    NavPoint {
        title: trim_inline(&title),
        epub_path: normalize_path(&join_path(base_directory, &no_fragment)),
        fragment: fragment(href),
        depth,
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
            fenced_code_block(&preformatted_text(node))
        ),
        "hr" => "---".to_string(),
        "table" => format!("{}{}", anchor_prefix(node), render_table(node, context)),
        "nav" => format!(
            "{}{}",
            anchor_prefix(node),
            render_navigation(node, context)
        ),
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
        "div" | "section" | "article" | "main" | "body" | "html" | "aside" => format!(
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
    let anchor = anchor_prefix(node);
    match node.tag_name().name() {
        "strong" | "b" => format!(
            "{anchor}**{}**",
            trim_inline(&render_inline_children(node, context))
        ),
        "em" | "i" => format!(
            "{anchor}*{}*",
            trim_inline(&render_inline_children(node, context))
        ),
        "code" => format!(
            "{anchor}`{}`",
            trim_inline(&render_inline_children(node, context)).replace('`', "\\`")
        ),
        "sup" => format!(
            "{anchor}<sup>{}</sup>",
            trim_inline(&render_inline_children(node, context))
        ),
        "sub" => format!(
            "{anchor}<sub>{}</sub>",
            trim_inline(&render_inline_children(node, context))
        ),
        "a" => {
            let text = trim_inline(&render_inline_children(node, context));
            let Some(href) = node.attribute("href").filter(|href| !href.is_empty()) else {
                return format!("{anchor}{text}");
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
            format!("{anchor}[{label}]({resolved})")
        }
        "img" => format!("{anchor}{}", render_image(node, context)),
        "br" => format!("{anchor}\n"),
        "li" | "ul" | "ol" => format!("{anchor}{}", render_block(node, context)),
        _ => format!("{anchor}{}", render_inline_children(node, context)),
    }
}

fn render_navigation(node: Node<'_, '_>, context: &RenderContext<'_>) -> String {
    let rendered = render_mixed_block_contents(node, context);
    if node.descendants().any(|descendant| {
        descendant.is_element() && matches!(descendant.tag_name().name(), "ol" | "ul")
    }) {
        return rendered;
    }

    let links = node
        .descendants()
        .filter(|descendant| descendant.is_element() && descendant.tag_name().name() == "a")
        .filter_map(|link| render_nav_link(link, context))
        .collect::<Vec<_>>();
    if links.len() < 2 {
        return rendered;
    }

    let mut blocks = Vec::new();
    if let Some(heading) = node.descendants().find(|descendant| {
        descendant.is_element()
            && matches!(
                descendant.tag_name().name(),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            )
    }) {
        let heading_text = trim_inline(&render_inline_children(heading, context));
        if !heading_text.is_empty() {
            let level = heading.tag_name().name()[1..]
                .parse::<usize>()
                .unwrap_or(2)
                .max(2);
            blocks.push(format!("{} {heading_text}", "#".repeat(level)));
        }
    }
    blocks.push(
        links
            .into_iter()
            .map(|link| format!("- {link}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    blocks.join("\n\n")
}

fn render_nav_link(node: Node<'_, '_>, context: &RenderContext<'_>) -> Option<String> {
    let href = node.attribute("href").filter(|href| !href.is_empty())?;
    let text = trim_inline(&render_inline_children(node, context));
    if text.is_empty() {
        return None;
    }
    let resolved = context
        .chapter_links
        .markdown_path(href, context.current_epub_path)
        .unwrap_or_else(|| href.to_string());
    Some(format!("[{text}]({resolved})"))
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

fn logical_chapter_segments(
    chapters: &[ChapterData],
    nav_points: &[NavPoint],
) -> Vec<LogicalChapterSegment> {
    if nav_points.is_empty() {
        return Vec::new();
    }
    let spine_index_by_path = chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| (chapter.item.absolute_path.clone(), index))
        .collect::<HashMap<_, _>>();
    let max_depth = nav_points
        .iter()
        .map(|point| point.depth)
        .max()
        .unwrap_or(1);
    let conservative = usable_logical_segments(build_logical_segments(
        nav_points,
        &spine_index_by_path,
        |point| {
            if max_depth > 1 {
                point.depth == 1
            } else {
                is_likely_logical_chapter_title(&point.title)
            }
        },
    ));
    if should_aggressively_split_nav(chapters, nav_points, &conservative) {
        let aggressive = usable_logical_segments(build_logical_segments(
            nav_points,
            &spine_index_by_path,
            |point| {
                point.fragment.is_some()
                    && if max_depth > 1 {
                        point.depth <= 2
                    } else {
                        is_likely_logical_chapter_title(&point.title)
                    }
            },
        ));
        if aggressive.len() > conservative.len() {
            return aggressive;
        }
    }
    conservative
}

fn build_logical_segments(
    nav_points: &[NavPoint],
    spine_index_by_path: &HashMap<String, usize>,
    mut split_here: impl FnMut(&NavPoint) -> bool,
) -> Vec<LogicalChapterSegment> {
    let mut used_names = HashSet::new();
    let mut segments = Vec::new();
    for (nav_index, point) in nav_points.iter().enumerate() {
        let Some(spine_index) = spine_index_by_path.get(&point.epub_path).copied() else {
            continue;
        };
        if !split_here(point) {
            continue;
        }
        let mut file_name =
            chapter_file_name(segments.len() + 1, Some(&point.title), &point.epub_path);
        if used_names.contains(&file_name) {
            let base = file_name.trim_end_matches(".md").to_string();
            let mut suffix = 2;
            while used_names.contains(&file_name) {
                file_name = format!("{base}-{suffix}.md");
                suffix += 1;
            }
        }
        used_names.insert(file_name.clone());
        segments.push(LogicalChapterSegment {
            title: point.title.clone(),
            file_name,
            nav_index,
            start: NavTarget {
                spine_index,
                fragment: point.fragment.clone(),
            },
        });
    }
    segments
}

fn usable_logical_segments(segments: Vec<LogicalChapterSegment>) -> Vec<LogicalChapterSegment> {
    if has_ambiguous_segment_boundaries(&segments) {
        return Vec::new();
    }
    if segments.len() < 2 {
        Vec::new()
    } else {
        segments
    }
}

fn should_aggressively_split_nav(
    chapters: &[ChapterData],
    nav_points: &[NavPoint],
    conservative_segments: &[LogicalChapterSegment],
) -> bool {
    if nav_points.len() < 12 || conservative_segments.len() > 4 {
        return false;
    }
    let max_spine_bytes = chapters
        .iter()
        .map(|chapter| chapter.data.len())
        .max()
        .unwrap_or(0);
    if max_spine_bytes < 160_000 {
        return false;
    }
    let mut nav_points_by_path: HashMap<&str, usize> = HashMap::new();
    for point in nav_points.iter().filter(|point| point.fragment.is_some()) {
        *nav_points_by_path.entry(&point.epub_path).or_default() += 1;
    }
    nav_points_by_path.values().any(|count| *count >= 12)
}

fn has_ambiguous_segment_boundaries(segments: &[LogicalChapterSegment]) -> bool {
    let mut previous_by_spine: HashMap<usize, &NavTarget> = HashMap::new();
    for segment in segments {
        let Some(previous) = previous_by_spine.insert(segment.start.spine_index, &segment.start)
        else {
            continue;
        };
        if segment.start.fragment.is_none() || previous.fragment == segment.start.fragment {
            return true;
        }
    }
    false
}

fn is_likely_logical_chapter_title(title: &str) -> bool {
    let title = trim_inline(title).replace('\u{3000}', " ");
    if title.is_empty() {
        return false;
    }
    let subsection_re = Regex::new(r"^\d+(?:\.\d+)+\s+").expect("valid regex");
    if subsection_re.is_match(&title) {
        return false;
    }
    let numbered_chapter_re = Regex::new(r"^\d+\s+\S").expect("valid regex");
    let cjk_chapter_re =
        Regex::new(r"^第[一二三四五六七八九十百千万\d]+[章节部分卷篇]").expect("valid regex");
    let named_chapter_re = Regex::new(
        r"^(附录|Appendix|Preface|Introduction|Foreword|Afterword|Part|Chapter|序言|前言|引言|作者介绍|后记|参考)",
    )
    .expect("valid regex");
    numbered_chapter_re.is_match(&title)
        || cjk_chapter_re.is_match(&title)
        || named_chapter_re.is_match(&title)
}

fn chapter_output_map(
    chapters: &[ChapterData],
    nav_points: &[NavPoint],
    segments: &[LogicalChapterSegment],
) -> HashMap<String, String> {
    if segments.is_empty() {
        return chapters
            .iter()
            .map(|chapter| {
                (
                    chapter.item.absolute_path.clone(),
                    chapter.file_name.clone(),
                )
            })
            .collect();
    }
    let mut output = HashMap::new();
    for (spine_index, chapter) in chapters.iter().enumerate() {
        if let Some(segment) = segment_for_spine_start(spine_index, segments) {
            output.insert(
                chapter.item.absolute_path.clone(),
                segment.file_name.clone(),
            );
        }
    }
    for (nav_index, point) in nav_points.iter().enumerate() {
        let Some(segment) = segment_for_nav_index(nav_index, segments) else {
            continue;
        };
        if let Some(fragment) = &point.fragment {
            output.insert(
                format!("{}#{fragment}", point.epub_path),
                format!("{}#{fragment}", segment.file_name),
            );
        } else {
            output.insert(point.epub_path.clone(), segment.file_name.clone());
        }
    }
    insert_fragment_output_paths(chapters, segments, &mut output);
    output
}

fn insert_fragment_output_paths(
    chapters: &[ChapterData],
    segments: &[LogicalChapterSegment],
    output: &mut HashMap<String, String>,
) {
    for (spine_index, chapter) in chapters.iter().enumerate() {
        let anchors = xhtml_anchor_ids(&chapter.data);
        if anchors.is_empty() {
            continue;
        }
        let anchor_position_by_id = anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| (anchor.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut segment_positions = segments
            .iter()
            .filter(|segment| segment.start.spine_index == spine_index)
            .map(|segment| {
                let position = segment
                    .start
                    .fragment
                    .as_ref()
                    .and_then(|fragment| anchor_position_by_id.get(fragment))
                    .copied()
                    .unwrap_or(0);
                (position, segment)
            })
            .collect::<Vec<_>>();
        segment_positions.sort_by_key(|(position, segment)| (*position, segment.nav_index));

        for (anchor_index, anchor) in anchors.iter().enumerate() {
            let segment = segment_positions
                .iter()
                .take_while(|(position, _)| *position <= anchor_index)
                .map(|(_, segment)| *segment)
                .last()
                .or_else(|| previous_segment_before_spine(spine_index, segments))
                .or_else(|| segments.first());
            let Some(segment) = segment else {
                continue;
            };
            output.insert(
                format!("{}#{anchor}", chapter.item.absolute_path),
                format!("{}#{anchor}", segment.file_name),
            );
        }
    }
}

fn previous_segment_before_spine(
    spine_index: usize,
    segments: &[LogicalChapterSegment],
) -> Option<&LogicalChapterSegment> {
    segments
        .iter()
        .take_while(|segment| segment.start.spine_index < spine_index)
        .last()
}

fn xhtml_anchor_ids(data: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Vec::new();
    };
    let content = strip_doctype(text);
    let Ok(doc) = Document::parse(&content) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for node in doc.descendants().filter(|node| node.is_element()) {
        let Some(id) = node
            .attribute("id")
            .or_else(|| node.attribute("name"))
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        if seen.insert(id.to_string()) {
            ids.push(id.to_string());
        }
    }
    ids
}

fn segment_for_spine_start(
    spine_index: usize,
    segments: &[LogicalChapterSegment],
) -> Option<&LogicalChapterSegment> {
    segments
        .iter()
        .take_while(|segment| segment.start.spine_index <= spine_index)
        .last()
        .or_else(|| segments.first())
}

fn segment_for_nav_index(
    nav_index: usize,
    segments: &[LogicalChapterSegment],
) -> Option<&LogicalChapterSegment> {
    segments
        .iter()
        .take_while(|segment| segment.nav_index <= nav_index)
        .last()
        .or_else(|| segments.first())
}

fn promote_navigation_targets_to_headings(
    mut markdown: String,
    epub_path: &str,
    nav_points: &[NavPoint],
) -> String {
    let max_depth = nav_points
        .iter()
        .map(|point| point.depth)
        .max()
        .unwrap_or(1);
    let mut seen_fragments = HashSet::new();
    for point in nav_points
        .iter()
        .filter(|point| point.epub_path == epub_path)
    {
        let level = if max_depth > 1 {
            point.depth
        } else if is_likely_logical_chapter_title(&point.title) {
            1
        } else {
            2
        };
        if let Some(fragment) = point.fragment.as_deref() {
            if !seen_fragments.insert(fragment.to_string()) {
                continue;
            }
            markdown =
                promote_anchor_to_heading(&markdown, fragment, &point.title, level.clamp(1, 6));
        } else if seen_fragments.insert(String::new()) {
            markdown = promote_start_to_heading(&markdown, &point.title, level.clamp(1, 6));
        }
    }
    markdown
}

fn promote_anchor_to_heading(markdown: &str, fragment: &str, title: &str, level: usize) -> String {
    let marker = format!(r#"<a id="{fragment}"></a>"#);
    let Some(marker_start) = markdown.find(&marker) else {
        return markdown.to_string();
    };
    let marker_end = marker_start + marker.len();
    let mut content_start = marker_end;
    while markdown[content_start..].starts_with('\n') || markdown[content_start..].starts_with(' ')
    {
        content_start += 1;
        if content_start >= markdown.len() {
            break;
        }
    }
    let heading = format!("{} {}", "#".repeat(level), trim_inline(title));
    if content_start >= markdown.len() {
        return format!("{markdown}\n\n{heading}");
    }
    let content_end = markdown[content_start..]
        .find("\n\n")
        .map(|offset| content_start + offset)
        .unwrap_or(markdown.len());
    let existing_block = markdown[content_start..content_end].trim();
    if existing_block.starts_with('#') {
        return markdown.to_string();
    }
    let mut output = String::new();
    output.push_str(&markdown[..marker_end]);
    output.push('\n');
    if title_comparable_text(existing_block) == title_comparable_text(title) {
        output.push_str(&heading);
        output.push_str(&markdown[content_end..]);
    } else {
        output.push_str(&heading);
        output.push_str("\n\n");
        output.push_str(&markdown[content_start..]);
    }
    output
}

fn promote_start_to_heading(markdown: &str, title: &str, level: usize) -> String {
    let mut content_start = 0;
    while markdown[content_start..].starts_with('\n') || markdown[content_start..].starts_with(' ')
    {
        content_start += 1;
        if content_start >= markdown.len() {
            break;
        }
    }
    let heading = format!("{} {}", "#".repeat(level), trim_inline(title));
    if content_start >= markdown.len() {
        return heading;
    }
    let content_end = markdown[content_start..]
        .find("\n\n")
        .map(|offset| content_start + offset)
        .unwrap_or(markdown.len());
    let existing_block = markdown[content_start..content_end].trim();
    if existing_block.starts_with('#') {
        return markdown.to_string();
    }
    let mut output = String::new();
    output.push_str(&markdown[..content_start]);
    if title_comparable_text(existing_block) == title_comparable_text(title) {
        output.push_str(&heading);
        output.push_str(&markdown[content_end..]);
    } else {
        output.push_str(&heading);
        output.push_str("\n\n");
        output.push_str(&markdown[content_start..]);
    }
    output
}

fn output_chapters_from_spines(
    chapters: &[ChapterData],
    converted_spines: &[String],
    segments: &[LogicalChapterSegment],
    nav_points: &[NavPoint],
    chapter_links: &ChapterLinkMap,
) -> Vec<(String, String, String)> {
    let mut output = if segments.is_empty() {
        chapters
            .iter()
            .zip(converted_spines.iter())
            .map(|(chapter, converted)| {
                (
                    chapter.file_name.clone(),
                    chapter
                        .title
                        .clone()
                        .unwrap_or_else(|| fallback_chapter_title(&chapter.file_name)),
                    converted.clone(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        let mut output = Vec::new();
        for (index, segment) in segments.iter().enumerate() {
            let next = segments.get(index + 1).map(|segment| &segment.start);
            let mut pieces = Vec::new();
            for spine_index in segment.start.spine_index
                ..=next
                    .map(|target| target.spine_index)
                    .unwrap_or_else(|| converted_spines.len().saturating_sub(1))
            {
                let Some(spine) = converted_spines.get(spine_index) else {
                    continue;
                };
                let start = if spine_index == segment.start.spine_index {
                    if index == 0 {
                        0
                    } else {
                        segment
                            .start
                            .fragment
                            .as_deref()
                            .and_then(|fragment| anchor_position(spine, fragment))
                            .unwrap_or(0)
                    }
                } else {
                    0
                };
                let end = if let Some(next) = next {
                    if spine_index == next.spine_index {
                        match next.fragment.as_deref() {
                            Some(fragment) => {
                                anchor_position(spine, fragment).unwrap_or(spine.len())
                            }
                            None => 0,
                        }
                    } else {
                        spine.len()
                    }
                } else {
                    spine.len()
                };
                if end <= start {
                    continue;
                }
                let piece = spine[start..end].trim();
                if !piece.is_empty() {
                    pieces.push(piece.to_string());
                }
            }
            let mut markdown = pieces.join("\n\n");
            if markdown.trim().is_empty() {
                markdown = format!("# {}", segment.title);
            }
            output.push((
                segment.file_name.clone(),
                segment.title.clone(),
                cleanup(&markdown),
            ));
        }
        output
    };
    let targets = plain_toc_link_targets(nav_points, chapter_links, &output);
    for (_, _, markdown) in &mut output {
        *markdown = link_plain_frontmatter_toc_entries(markdown, &targets);
    }
    output
}

fn link_plain_frontmatter_toc_entries(
    markdown: &str,
    targets: &HashMap<String, Option<String>>,
) -> String {
    if targets.is_empty() {
        return markdown.to_string();
    }

    let mut seen_primary_heading = false;
    let mut active_toc_block = false;
    let mut list_items_seen = 0usize;
    let mut done = false;
    let mut current_top_level_target: Option<String> = None;
    let mut lines = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();
        if done {
            lines.push(line.to_string());
            continue;
        }

        if !seen_primary_heading {
            seen_primary_heading = trimmed.starts_with("# ");
            lines.push(line.to_string());
            continue;
        }

        if active_toc_block {
            if is_markdown_list_line(line) {
                list_items_seen += 1;
                let linked = link_plain_toc_list_line(line, targets)
                    .or_else(|| fallback_parent_toc_list_line(line, &current_top_level_target));
                if list_line_indent(line) == 0 {
                    if let Some(target) = linked.as_deref().and_then(markdown_link_target) {
                        current_top_level_target = Some(target);
                    }
                }
                lines.push(linked.unwrap_or_else(|| line.to_string()));
                continue;
            }
            if trimmed.is_empty() || is_standalone_anchor_line(trimmed) {
                lines.push(line.to_string());
                continue;
            }
            if is_markdown_heading_line(trimmed) {
                done = true;
                lines.push(line.to_string());
                continue;
            }
            if list_items_seen >= 3 {
                done = true;
            } else {
                active_toc_block = false;
                list_items_seen = 0;
            }
            lines.push(line.to_string());
            continue;
        }

        if is_markdown_subheading_line(trimmed) {
            done = true;
            lines.push(line.to_string());
            continue;
        }

        if is_markdown_list_line(line) {
            active_toc_block = true;
            list_items_seen = 1;
            let linked = link_plain_toc_list_line(line, targets);
            if list_line_indent(line) == 0 {
                if let Some(target) = linked.as_deref().and_then(markdown_link_target) {
                    current_top_level_target = Some(target);
                }
            }
            lines.push(linked.unwrap_or_else(|| line.to_string()));
        } else {
            lines.push(line.to_string());
        }
    }

    let mut linked = lines.join("\n");
    if markdown.ends_with('\n') {
        linked.push('\n');
    }
    linked
}

fn plain_toc_link_targets(
    nav_points: &[NavPoint],
    chapter_links: &ChapterLinkMap,
    output_chapters: &[(String, String, String)],
) -> HashMap<String, Option<String>> {
    let mut targets = HashMap::new();
    for point in nav_points {
        let Some(link) = nav_point_markdown_link(point, chapter_links) else {
            continue;
        };
        insert_unique_toc_target(&mut targets, normalize_toc_lookup_text(&point.title), &link);
        let stripped = strip_leading_section_number(&point.title);
        insert_unique_toc_target(&mut targets, normalize_toc_lookup_text(&stripped), &link);
    }
    insert_heading_toc_targets(&mut targets, output_chapters);
    targets
        .into_iter()
        .filter(|(key, value)| !key.is_empty() && value.is_some())
        .collect()
}

fn insert_heading_toc_targets(
    targets: &mut HashMap<String, Option<String>>,
    output_chapters: &[(String, String, String)],
) {
    for (file_name, _, markdown) in output_chapters {
        let mut pending_anchor: Option<String> = None;
        for line in markdown.lines() {
            let trimmed = line.trim();
            if let Some(anchor) = standalone_anchor_ids(trimmed).last().cloned() {
                pending_anchor = Some(anchor);
                continue;
            }
            if let Some(title) = markdown_heading_title(trimmed) {
                let link = pending_anchor
                    .as_deref()
                    .map(|anchor| format!("{file_name}#{anchor}"))
                    .unwrap_or_else(|| file_name.clone());
                insert_preferred_toc_target(targets, normalize_toc_lookup_text(&title), &link);
                let stripped = strip_leading_section_number(&title);
                insert_preferred_toc_target(targets, normalize_toc_lookup_text(&stripped), &link);
            }
            if !trimmed.is_empty() {
                pending_anchor = None;
            }
        }
    }
}

fn nav_point_markdown_link(point: &NavPoint, chapter_links: &ChapterLinkMap) -> Option<String> {
    if let Some(fragment) = point.fragment.as_deref() {
        let key = format!("{}#{fragment}", point.epub_path);
        if let Some(link) = chapter_links.epub_path_to_markdown.get(&key) {
            return Some(link.clone());
        }
        if let Some(markdown) = chapter_links.epub_path_to_markdown.get(&point.epub_path) {
            return Some(format!("{markdown}#{fragment}"));
        }
    }
    chapter_links
        .epub_path_to_markdown
        .get(&point.epub_path)
        .cloned()
}

fn insert_unique_toc_target(
    targets: &mut HashMap<String, Option<String>>,
    key: String,
    link: &str,
) {
    if key.is_empty() {
        return;
    }
    match targets.get_mut(&key) {
        Some(existing) if existing.as_deref() == Some(link) => {}
        Some(existing) => *existing = None,
        None => {
            targets.insert(key, Some(link.to_string()));
        }
    }
}

fn insert_preferred_toc_target(
    targets: &mut HashMap<String, Option<String>>,
    key: String,
    link: &str,
) {
    if key.is_empty() || targets.contains_key(&key) {
        return;
    }
    targets.insert(key, Some(link.to_string()));
}

fn link_plain_toc_list_line(
    line: &str,
    targets: &HashMap<String, Option<String>>,
) -> Option<String> {
    let captures = markdown_list_line_captures(line)?;
    let label = captures.get(3)?.as_str().trim();
    if label.is_empty() || label.contains("](") {
        return None;
    }
    let (anchor_prefix, link_label) = split_leading_inline_anchors(label);
    let normalized = normalize_toc_lookup_text(link_label);
    let marker = captures.get(2).map_or("", |value| value.as_str()).trim();
    let numbered_label = if marker
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        Some(format!("{marker} {link_label}"))
    } else {
        None
    };
    let target = targets
        .get(
            &numbered_label
                .as_deref()
                .map(normalize_toc_lookup_text)
                .unwrap_or_default(),
        )
        .and_then(|value| value.as_ref())
        .or_else(|| targets.get(&normalized).and_then(|value| value.as_ref()))
        .or_else(|| {
            let stripped = strip_leading_section_number(link_label);
            targets
                .get(&normalize_toc_lookup_text(&stripped))
                .and_then(|value| value.as_ref())
        })?;
    Some(format!(
        "{}{}{}[{}]({})",
        captures.get(1).map_or("", |value| value.as_str()),
        captures.get(2).map_or("", |value| value.as_str()),
        anchor_prefix,
        escape_markdown_link_text(link_label),
        target
    ))
}

fn fallback_parent_toc_list_line(line: &str, parent_target: &Option<String>) -> Option<String> {
    if list_line_indent(line) == 0 {
        return None;
    }
    let target = parent_target.as_ref()?;
    let captures = markdown_list_line_captures(line)?;
    let label = captures.get(3)?.as_str().trim();
    if label.is_empty() || label.contains("](") {
        return None;
    }
    let (anchor_prefix, link_label) = split_leading_inline_anchors(label);
    Some(format!(
        "{}{}{}[{}]({})",
        captures.get(1).map_or("", |value| value.as_str()),
        captures.get(2).map_or("", |value| value.as_str()),
        anchor_prefix,
        escape_markdown_link_text(link_label),
        target
    ))
}

fn markdown_link_target(line: &str) -> Option<String> {
    let captures = Regex::new(r"\]\(([^)]+)\)")
        .expect("valid regex")
        .captures(line)?;
    Some(captures.get(1)?.as_str().to_string())
}

fn list_line_indent(line: &str) -> usize {
    markdown_list_line_captures(line)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().len()))
        .unwrap_or(0)
}

fn is_markdown_list_line(line: &str) -> bool {
    markdown_list_line_captures(line).is_some()
}

fn markdown_list_line_captures(line: &str) -> Option<regex::Captures<'_>> {
    Regex::new(r"^(\s*)([-*+]\s+|\d+[.)]\s+)(.+?)\s*$")
        .expect("valid regex")
        .captures(line)
}

fn split_leading_inline_anchors(label: &str) -> (String, &str) {
    let mut anchors = String::new();
    let mut rest = label.trim_start();
    loop {
        let trimmed = rest.trim_start();
        if !trimmed.starts_with("<a ") {
            return (anchors, trimmed);
        }
        let Some(end) = trimmed.find("</a>") else {
            return (anchors, rest);
        };
        let end = end + "</a>".len();
        anchors.push_str(&trimmed[..end]);
        anchors.push(' ');
        rest = &trimmed[end..];
    }
}

fn normalize_toc_lookup_text(text: &str) -> String {
    let without_tags = Regex::new(r"<[^>]+>")
        .expect("valid regex")
        .replace_all(text, " ");
    let without_markdown = without_tags
        .replace(['`', '*', '_'], "")
        .replace("\\[", "[")
        .replace("\\]", "]");
    Regex::new(r"\s+")
        .expect("valid regex")
        .replace_all(&without_markdown, " ")
        .trim()
        .trim_matches(|char: char| matches!(char, '.' | ':' | ';' | '-' | '–' | '—'))
        .to_lowercase()
}

fn strip_leading_section_number(text: &str) -> String {
    Regex::new(r"^\s*\d+(?:\.\d+)*\.?\s+")
        .expect("valid regex")
        .replace(text, "")
        .to_string()
}

fn escape_markdown_link_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn is_standalone_anchor_line(line: &str) -> bool {
    !standalone_anchor_ids(line).is_empty()
}

fn standalone_anchor_ids(line: &str) -> Vec<String> {
    let wrapper = Regex::new(r#"^(?:<a\s+id="[^"]+"></a>\s*)+$"#).expect("valid regex");
    if !wrapper.is_match(line) {
        return Vec::new();
    }
    Regex::new(r#"<a\s+id="([^"]+)"></a>"#)
        .expect("valid regex")
        .captures_iter(line)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .collect()
}

fn markdown_heading_title(line: &str) -> Option<String> {
    let captures = Regex::new(r"^#{1,6}\s+(.+?)\s*$")
        .expect("valid regex")
        .captures(line)?;
    Some(captures.get(1)?.as_str().trim().to_string())
}

fn is_markdown_heading_line(line: &str) -> bool {
    Regex::new(r"^#{1,6}\s+")
        .expect("valid regex")
        .is_match(line)
}

fn is_markdown_subheading_line(line: &str) -> bool {
    Regex::new(r"^#{2,6}\s+")
        .expect("valid regex")
        .is_match(line)
}

fn anchor_position(markdown: &str, fragment: &str) -> Option<usize> {
    markdown.find(&format!(r#"<a id="{fragment}"></a>"#))
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

fn metadata_json(package: &EpubPackage, cover_asset_path: Option<&str>) -> String {
    let cover = cover_asset_path
        .map(|path| format!("\"{}\"", json_escape(path)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\n  \"title\": \"{}\",\n  \"cover_asset_path\": {}\n}}\n",
        json_escape(&package.title),
        cover
    )
}

fn json_escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output
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

fn preformatted_text(node: Node<'_, '_>) -> String {
    let mut text = String::new();
    append_preformatted_text(node, &mut text);
    text
}

fn append_preformatted_text(node: Node<'_, '_>, output: &mut String) {
    for child in node.children() {
        match child.node_type() {
            NodeType::Text => output.push_str(child.text().unwrap_or("")),
            NodeType::Element if child.tag_name().name() == "br" => output.push('\n'),
            NodeType::Element => append_preformatted_text(child, output),
            _ => {}
        }
    }
}

fn fenced_code_block(code: &str) -> String {
    let fence = code_fence_for(code);
    let closing_separator = if code.ends_with('\n') { "" } else { "\n" };
    format!("{fence}\n{code}{closing_separator}{fence}")
}

fn code_fence_for(code: &str) -> String {
    let longest_backtick_run = longest_repeated_char_run(code, '`');
    "`".repeat((longest_backtick_run + 1).max(3))
}

fn longest_repeated_char_run(value: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in value.chars() {
        if character == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
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
    let (protected, code_blocks) = protect_fenced_code_blocks(markdown);
    let value = space_re.replace_all(&protected, " ");
    let value = structure_flat_toc_blocks(&value);
    let value = newline_re.replace_all(&value, "\n\n");
    let value = remove_duplicate_leading_title_blocks(&value);
    let value = restore_fenced_code_blocks(&value, &code_blocks);
    format!("{}\n", value.trim())
}

fn protect_fenced_code_blocks(markdown: &str) -> (String, Vec<String>) {
    let mut protected = String::new();
    let mut code_blocks = Vec::new();
    let mut active_fence: Option<(char, usize)> = None;
    let mut active_block = String::new();

    for line in markdown.split_inclusive('\n') {
        if let Some((fence_char, fence_len)) = active_fence {
            active_block.push_str(line);
            if is_closing_fence_line(line, fence_char, fence_len) {
                let placeholder = fenced_code_placeholder(code_blocks.len());
                code_blocks.push(std::mem::take(&mut active_block));
                protected.push_str(&placeholder);
                active_fence = None;
            }
            continue;
        }

        if let Some((fence_char, fence_len)) = opening_fence_line(line) {
            active_block.push_str(line);
            active_fence = Some((fence_char, fence_len));
        } else {
            protected.push_str(line);
        }
    }

    if !active_block.is_empty() {
        protected.push_str(&active_block);
    }

    (protected, code_blocks)
}

fn restore_fenced_code_blocks(markdown: &str, code_blocks: &[String]) -> String {
    let mut restored = markdown.to_string();
    for (index, code_block) in code_blocks.iter().enumerate() {
        restored = restored.replace(&fenced_code_placeholder(index), code_block);
    }
    restored
}

fn fenced_code_placeholder(index: usize) -> String {
    format!("\u{E000}EPUBMD_FENCED_CODE_BLOCK_{index}\u{E000}")
}

fn opening_fence_line(line: &str) -> Option<(char, usize)> {
    let trimmed = trim_line_ending(line).trim_start();
    let indent = trim_line_ending(line).len() - trim_line_ending(line).trim_start().len();
    if indent > 3 {
        return None;
    }
    let fence_char = trimmed
        .chars()
        .next()
        .filter(|char| matches!(char, '`' | '~'))?;
    let fence_len = trimmed
        .chars()
        .take_while(|char| *char == fence_char)
        .count();
    if fence_len >= 3 {
        Some((fence_char, fence_len))
    } else {
        None
    }
}

fn is_closing_fence_line(line: &str, fence_char: char, opening_len: usize) -> bool {
    let trimmed = trim_line_ending(line).trim_start();
    let indent = trim_line_ending(line).len() - trim_line_ending(line).trim_start().len();
    if indent > 3 {
        return false;
    }
    let fence_len = trimmed
        .chars()
        .take_while(|char| *char == fence_char)
        .count();
    fence_len >= opening_len && trimmed[fence_len..].trim().is_empty()
}

fn trim_line_ending(line: &str) -> &str {
    line.trim_end_matches('\n').trim_end_matches('\r')
}

fn structure_flat_toc_blocks(markdown: &str) -> String {
    let mut previous_was_toc_heading = false;
    let mut blocks = Vec::new();
    for block in markdown.split("\n\n") {
        let structured = structure_flat_toc_block(block, previous_was_toc_heading);
        previous_was_toc_heading = is_toc_heading_block(block);
        if previous_was_toc_heading && structured != block {
            previous_was_toc_heading = false;
        }
        blocks.push(structured);
    }
    blocks.join("\n\n")
}

fn structure_flat_toc_block(block: &str, previous_was_toc_heading: bool) -> String {
    let trimmed = block.trim();
    if let Some((heading, body)) = toc_heading_block_and_body(trimmed) {
        let entries = split_flat_toc_entries(&body);
        if entries.len() < 3 {
            return block.to_string();
        }
        return format!("{heading}\n\n{}", format_toc_entries(entries));
    }
    if let Some((heading, body)) = flat_toc_heading_and_body(trimmed) {
        let entries = split_flat_toc_entries(&unwrap_toc_paragraph(body));
        if entries.len() < 3 {
            return block.to_string();
        }
        return format!("## {heading}\n\n{}", format_toc_entries(entries));
    }
    if previous_was_toc_heading {
        let entries = split_flat_toc_entries(&unwrap_toc_paragraph(trimmed));
        if entries.len() >= 3 {
            return format_toc_entries(entries);
        }
    }
    block.to_string()
}

fn format_toc_entries(entries: Vec<String>) -> String {
    entries
        .into_iter()
        .map(|entry| format!("- {}", normalize_toc_entry(&entry)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_toc_entry(entry: &str) -> String {
    Regex::new(r#"\[((?:\s*<a\s+id="[^"]+"></a>)+\s*)([^\]]+?)\]\(([^)]+)\)"#)
        .expect("valid regex")
        .replace_all(entry, |captures: &regex::Captures<'_>| {
            format!(
                "{} [{}]({})",
                captures[1].trim(),
                captures[2].trim(),
                captures[3].trim()
            )
        })
        .to_string()
}

fn toc_heading_block_and_body(block: &str) -> Option<(String, String)> {
    let mut lines = block.lines();
    let first_line = lines.next()?.trim();
    let body = lines.collect::<Vec<_>>().join("\n");
    if body.trim().is_empty() || !is_toc_heading_block(first_line) {
        return None;
    }
    Some((first_line.to_string(), unwrap_toc_paragraph(&body)))
}

fn flat_toc_heading_and_body(block: &str) -> Option<(&'static str, &str)> {
    let lower = block.to_lowercase();
    if lower == "contents" || lower == "table of contents" {
        return None;
    }
    if lower.starts_with("table of contents ") {
        return Some((
            "Table of Contents",
            block["table of contents".len()..].trim(),
        ));
    }
    if lower.starts_with("contents ") {
        return Some(("Contents", block["contents".len()..].trim()));
    }
    None
}

fn is_toc_heading_block(block: &str) -> bool {
    let heading = Regex::new(r"^#{1,6}\s+").expect("valid regex");
    let tag = Regex::new(r"<[^>]+>").expect("valid regex");
    let text = heading.replace(block.trim(), "");
    let text = tag.replace_all(&text, "");
    matches!(
        text.trim().to_lowercase().as_str(),
        "contents" | "table of contents"
    )
}

fn split_flat_toc_entries(body: &str) -> Vec<String> {
    let normalized_body = unwrap_toc_paragraph(body).replace("&nbsp;", " ");
    let markers = flat_toc_marker_starts(&normalized_body);
    if markers.len() < 3 {
        return Vec::new();
    }

    markers
        .iter()
        .enumerate()
        .filter_map(|(index, start)| {
            let end = markers
                .get(index + 1)
                .copied()
                .unwrap_or(normalized_body.len());
            let entry = normalized_body[*start..end].trim();
            if entry.is_empty() {
                None
            } else {
                Some(entry.to_string())
            }
        })
        .collect()
}

fn unwrap_toc_paragraph(value: &str) -> String {
    let trimmed = value.trim();
    Regex::new(r"(?is)^<p(?:\s[^>]*)?>(.*)</p>$")
        .expect("valid regex")
        .captures(trimmed)
        .and_then(|captures| captures.get(1).map(|body| body.as_str().trim().to_string()))
        .unwrap_or_else(|| trimmed.to_string())
}

fn flat_toc_marker_starts(body: &str) -> Vec<usize> {
    body.char_indices()
        .filter_map(|(index, character)| {
            if character.is_ascii_digit() && is_flat_toc_marker_at(body, index) {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

fn is_flat_toc_marker_at(body: &str, index: usize) -> bool {
    let bytes = body.as_bytes();
    if index > 0 && !bytes[index - 1].is_ascii_whitespace() {
        return false;
    }

    let mut cursor = index;
    let mut saw_digit = false;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        saw_digit = true;
        cursor += 1;
    }
    if !saw_digit {
        return false;
    }

    while cursor < bytes.len() && bytes[cursor] == b'.' {
        cursor += 1;
        let group_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if group_start == cursor {
            return false;
        }
    }

    cursor < bytes.len() && bytes[cursor].is_ascii_whitespace()
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
    fn preserves_preformatted_code_exactly() {
        let markdown = convert_xhtml_to_markdown(
            b"<html><body><p>Before.</p><pre><code>fn main() {\n    let label = \"a   b\";\n\tprintln!(\"{}\", label);\n}\n</code></pre><p>After.</p></body></html>",
            "OPS/code.xhtml",
            &AssetMapper::default(),
            &ChapterLinkMap {
                epub_path_to_markdown: HashMap::new(),
            },
        )
        .expect("convert code");

        assert!(markdown.contains(
            "```\nfn main() {\n    let label = \"a   b\";\n\tprintln!(\"{}\", label);\n}\n```\n"
        ));
    }

    #[test]
    fn preserves_preformatted_blank_lines_and_br_elements() {
        let markdown = convert_xhtml_to_markdown(
            b"<html><body><pre>line 1<br/>    line 2\n\n\tline 4</pre></body></html>",
            "OPS/code.xhtml",
            &AssetMapper::default(),
            &ChapterLinkMap {
                epub_path_to_markdown: HashMap::new(),
            },
        )
        .expect("convert pre");

        assert_eq!(markdown, "```\nline 1\n    line 2\n\n\tline 4\n```\n");
    }

    #[test]
    fn uses_longer_fence_when_code_contains_backticks() {
        assert_eq!(
            fenced_code_block("let markdown = \"```\";\nlet more = \"````\";"),
            "`````\nlet markdown = \"```\";\nlet more = \"````\";\n`````"
        );
    }

    #[test]
    fn cleanup_does_not_rewrite_fenced_code_blocks() {
        let cleaned = cleanup("Intro\n\n```\nlet x =  1;\n\n    let y =\t2;\n```\n\nDone");

        assert!(cleaned.contains("```\nlet x =  1;\n\n    let y =\t2;\n```"));
    }

    #[test]
    fn renders_flat_nav_links_as_toc_list() {
        let mut chapter_links = HashMap::new();
        chapter_links.insert("OPS/chapter1.xhtml".to_string(), "001-Intro.md".to_string());
        chapter_links.insert(
            "OPS/chapter1.xhtml#overview".to_string(),
            "001-Intro.md#overview".to_string(),
        );
        let markdown = convert_xhtml_to_markdown(
            br#"<html><body><nav><h1>Contents</h1><a href="chapter1.xhtml">1 Introduction to Fennel and Lua</a><a href="chapter1.xhtml#overview">1.1 Overview of Lua Language</a><a href="chapter2.xhtml">2 Setting Up Your Fennel Environment</a></nav></body></html>"#,
            "OPS/nav.xhtml",
            &AssetMapper::default(),
            &ChapterLinkMap { epub_path_to_markdown: chapter_links },
        )
        .expect("convert nav");

        assert!(markdown.contains("## Contents"));
        assert!(markdown.contains("- [1 Introduction to Fennel and Lua](001-Intro.md)"));
        assert!(markdown.contains("- [1.1 Overview of Lua Language](001-Intro.md#overview)"));
        assert!(markdown.contains("- [2 Setting Up Your Fennel Environment](chapter2.xhtml)"));
        assert!(!markdown.contains("Lua 1.1 Overview"));
    }

    #[test]
    fn structures_flat_plain_text_toc() {
        let markdown = cleanup("Contents 1 Introduction to Fennel and Lua 1.1 Overview of Lua Language 1.2 Fennel as a Lisp for Lua 2 Setting Up Your Fennel Environment 2.1 Installing Lua");

        assert!(markdown.contains("## Contents"));
        assert!(markdown.contains("- 1 Introduction to Fennel and Lua"));
        assert!(markdown.contains("- 1.1 Overview of Lua Language"));
        assert!(markdown.contains("- 2 Setting Up Your Fennel Environment"));
        assert!(!markdown.contains("Lua 1.1 Overview"));
    }

    #[test]
    fn structures_flat_toc_paragraph_after_contents_heading() {
        let markdown = cleanup("# Contents\n\n1 [Introduction to Fennel and Lua](chapter-1.md) 1.1 [Overview of Lua Language](chapter-1.md#overview) 1.2 [Fennel as a Lisp for Lua](chapter-1.md#fennel) 2 [Setting Up Your Fennel Environment](chapter-2.md)");

        assert!(markdown.contains("# Contents"));
        assert!(markdown.contains("- 1 [Introduction to Fennel and Lua](chapter-1.md)"));
        assert!(markdown.contains("- 1.1 [Overview of Lua Language](chapter-1.md#overview)"));
        assert!(markdown.contains("- 2 [Setting Up Your Fennel Environment](chapter-2.md)"));
        assert!(!markdown.contains("Lua](chapter-1.md) 1.1"));
    }

    #[test]
    fn structures_flat_toc_after_single_newline_heading() {
        let markdown = cleanup("# Contents\n1 [Introduction to Fennel and Lua](chapter-1.md) 1.1 [Overview of Lua Language](chapter-1.md#overview) 1.2 [Fennel as a Lisp for Lua](chapter-1.md#fennel) 2 [Setting Up Your Fennel Environment](chapter-2.md)");

        assert!(markdown.contains("# Contents"));
        assert!(markdown.contains("- 1 [Introduction to Fennel and Lua](chapter-1.md)"));
        assert!(markdown.contains("- 1.1 [Overview of Lua Language](chapter-1.md#overview)"));
        assert!(!markdown.contains("Lua](chapter-1.md) 1.1"));
    }

    #[test]
    fn structures_flat_html_paragraph_toc() {
        let markdown = cleanup("# Contents\n<p>1 <a href=\"chapter-1.md\">Introduction to Fennel and Lua</a> 1.1 <a href=\"chapter-1.md#overview\">Overview of Lua Language</a> 1.2 <a href=\"chapter-1.md#fennel\">Fennel as a Lisp for Lua</a> 2 <a href=\"chapter-2.md\">Setting Up Your Fennel Environment</a></p>");

        assert!(
            markdown.contains("- 1 <a href=\"chapter-1.md\">Introduction to Fennel and Lua</a>")
        );
        assert!(markdown
            .contains("- 1.1 <a href=\"chapter-1.md#overview\">Overview of Lua Language</a>"));
        assert!(!markdown.contains("- <p>1"));
    }

    #[test]
    fn moves_inline_anchors_out_of_toc_link_labels() {
        let markdown = cleanup("# Contents\n\n1 <a id=\"QQ2-4-3\"></a> [<a id=\"kobo.3.1\"></a> Introduction to Fennel and Lua](chapter-1.md) 1.1 <a id=\"QQ2-4-4\"></a> [<a id=\"kobo.5.1\"></a> Overview of Lua Language](chapter-1.md#overview) 2 <a id=\"QQ2-5-9\"></a> [<a id=\"kobo.7.1\"></a> Setting Up Your Fennel Environment](chapter-2.md)");

        assert!(!markdown.contains("[<a id=\"kobo.3.1\"></a> Introduction"));
        assert!(markdown
            .contains("<a id=\"kobo.3.1\"></a> [Introduction to Fennel and Lua](chapter-1.md)"));
    }

    #[test]
    fn links_plain_frontmatter_toc_entries_from_navigation() {
        let nav_points = vec![
            NavPoint {
                title: "From the Author".to_string(),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some("from-the-author".to_string()),
                depth: 1,
            },
            NavPoint {
                title: "1. How Multiparadigm Is Expanding Modern Languages".to_string(),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some("1-how-multiparadigm-is-expanding-modern-languages".to_string()),
                depth: 1,
            },
            NavPoint {
                title: "1.1 The Iterator Pattern in OOP and First-Class Functions".to_string(),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some(
                    "11-the-iterator-pattern-in-oop-and-first-class-functions".to_string(),
                ),
                depth: 2,
            },
            NavPoint {
                title: "1.6 Summary".to_string(),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some("16-summary".to_string()),
                depth: 2,
            },
            NavPoint {
                title: "3.5 Summary".to_string(),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some("35-summary".to_string()),
                depth: 2,
            },
        ];
        let chapter_links = ChapterLinkMap {
            epub_path_to_markdown: HashMap::from([
                (
                    "EPUB/text/all.xhtml#from-the-author".to_string(),
                    "001-Multi-Paradigm-Programming.md#from-the-author".to_string(),
                ),
                (
                    "EPUB/text/all.xhtml#1-how-multiparadigm-is-expanding-modern-languages"
                        .to_string(),
                    "001-Multi-Paradigm-Programming.md#1-how-multiparadigm-is-expanding-modern-languages"
                        .to_string(),
                ),
                (
                    "EPUB/text/all.xhtml#11-the-iterator-pattern-in-oop-and-first-class-functions"
                        .to_string(),
                    "001-Multi-Paradigm-Programming.md#11-the-iterator-pattern-in-oop-and-first-class-functions"
                        .to_string(),
                ),
                (
                    "EPUB/text/all.xhtml#16-summary".to_string(),
                    "001-Multi-Paradigm-Programming.md#16-summary".to_string(),
                ),
                (
                    "EPUB/text/all.xhtml#35-summary".to_string(),
                    "001-Multi-Paradigm-Programming.md#35-summary".to_string(),
                ),
            ]),
        };
        let markdown = "# Multi-Paradigm Programming\n\nCombining Functional, Object-Oriented, and Lisp Paradigms for Software Design and Implementation\n\n<a id=\"section-1\"></a>\n- From the Author\n\n<a id=\"section-2\"></a>\n1. How Multiparadigm Is Expanding Modern Languages\n   1. The Iterator Pattern in OOP and First-Class Functions\n   1. GoF’s Iterator Pattern\n   1. Summary\n\n<a id=\"from-the-author\"></a>\n## From the Author\n\n1. The Iterator Pattern in OOP and First-Class Functions\n\n<a id=\"gofs-iterator-pattern\"></a>\n### GoF’s Iterator Pattern\n";
        let output_chapters = vec![(
            "001-Multi-Paradigm-Programming.md".to_string(),
            "Multi-Paradigm Programming".to_string(),
            markdown.to_string(),
        )];
        let targets = plain_toc_link_targets(&nav_points, &chapter_links, &output_chapters);

        let linked = link_plain_frontmatter_toc_entries(markdown, &targets);

        assert!(linked
            .contains("- [From the Author](001-Multi-Paradigm-Programming.md#from-the-author)"));
        assert!(linked.contains("1. [How Multiparadigm Is Expanding Modern Languages](001-Multi-Paradigm-Programming.md#1-how-multiparadigm-is-expanding-modern-languages)"));
        assert!(linked.contains("   1. [The Iterator Pattern in OOP and First-Class Functions](001-Multi-Paradigm-Programming.md#11-the-iterator-pattern-in-oop-and-first-class-functions)"));
        assert!(linked.contains(
            "   1. [GoF’s Iterator Pattern](001-Multi-Paradigm-Programming.md#gofs-iterator-pattern)"
        ));
        assert!(linked.contains("   1. [Summary](001-Multi-Paradigm-Programming.md#1-how-multiparadigm-is-expanding-modern-languages)"));
        assert!(linked.contains(
            "## From the Author\n\n1. The Iterator Pattern in OOP and First-Class Functions"
        ));
    }

    #[test]
    fn aggressively_splits_large_nav_packed_single_spine_books() {
        let chapters = vec![ChapterData {
            item: ManifestItem {
                href: "all.xhtml".to_string(),
                media_type: "application/xhtml+xml".to_string(),
                absolute_path: "EPUB/text/all.xhtml".to_string(),
                properties: String::new(),
            },
            data: vec![b'x'; 200_000],
            file_name: "001-All.md".to_string(),
            title: Some("All".to_string()),
        }];
        let mut nav_points = vec![NavPoint {
            title: "Book Root".to_string(),
            epub_path: "EPUB/text/all.xhtml".to_string(),
            fragment: Some("root".to_string()),
            depth: 1,
        }];
        for index in 1..=14 {
            nav_points.push(NavPoint {
                title: format!("{index}. Section {index}"),
                epub_path: "EPUB/text/all.xhtml".to_string(),
                fragment: Some(format!("section-{index}")),
                depth: 2,
            });
        }

        let segments = logical_chapter_segments(&chapters, &nav_points);

        assert!(segments.len() > 4);
        assert!(segments
            .iter()
            .any(|segment| segment.title == "1. Section 1"));
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
