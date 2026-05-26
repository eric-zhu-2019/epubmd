use epubmd_core::{convert_epub_to_zip, ConversionError};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use tempfile::tempdir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[test]
fn converts_synthetic_epub_to_reader_zip() {
    let temp = tempdir().unwrap();
    let epub = temp.path().join("sample.epub");
    make_epub(&epub, true, true, false);
    let output = temp.path().join("sample.zip");

    convert_epub_to_zip(&epub, &output, false).unwrap();

    let mut zip = ZipArchive::new(File::open(&output).unwrap()).unwrap();
    assert!(zip.by_name("README.md").is_ok());
    assert!(zip.by_name("style.css").is_ok());
    assert!(zip.by_name("chapters/001-Opening-Chapter.md").is_ok());
    assert!(zip.by_name("assets/OEBPS-images-pic.png").is_ok());

    let chapter = read_zip_text(&mut zip, "chapters/001-Opening-Chapter.md");
    assert!(chapter.contains("# Opening Chapter"));
    assert_eq!(
        chapter.matches("Opening Chapter").count(),
        1,
        "duplicate leading title should be removed"
    );
    assert!(chapter.contains("![Picture](../assets/OEBPS-images-pic.png)"));
    assert!(chapter.contains("[Second](002-Second-Chapter.md#next)"));
}

#[test]
fn resolves_parent_directory_assets_and_undeclared_existing_assets() {
    let temp = tempdir().unwrap();
    let epub = temp.path().join("sample.epub");
    make_epub(&epub, false, true, true);
    let output = temp.path().join("sample.zip");

    convert_epub_to_zip(&epub, &output, false).unwrap();

    let mut zip = ZipArchive::new(File::open(&output).unwrap()).unwrap();
    let chapter = read_zip_text(&mut zip, "chapters/001-Opening-Chapter.md");
    assert!(chapter.contains("![Picture](../assets/OEBPS-media-pic.png)"));
    assert!(zip.by_name("assets/OEBPS-media-pic.png").is_ok());
}

#[test]
fn splits_flat_navigation_anchors_into_logical_chapters_across_spine_files() {
    let temp = tempdir().unwrap();
    let epub = temp.path().join("mixed-navigation.epub");
    make_mixed_navigation_epub(&epub);
    let output = temp.path().join("mixed-navigation.zmd");

    convert_epub_to_zip(&epub, &output, false).unwrap();

    let mut zip = ZipArchive::new(File::open(&output).unwrap()).unwrap();
    let mut chapter_names = zip
        .file_names()
        .filter(|name| name.starts_with("chapters/") && name.ends_with(".md"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    chapter_names.sort();

    assert_eq!(
        chapter_names,
        vec![
            "chapters/001-Preface.md",
            "chapters/002-Chapter-1-Start.md",
            "chapters/003-Chapter-2-Next.md",
        ]
    );

    let first = read_zip_text(&mut zip, "chapters/001-Preface.md");
    assert!(first.contains("# Preface"));
    assert!(first.contains("[Chapter 1](002-Chapter-1-Start.md#c1)"));
    assert!(first.contains("[Chapter 2](003-Chapter-2-Next.md#c2)"));
    assert!(first.contains("[Deep target](003-Chapter-2-Next.md#deep)"));

    let chapter_one = read_zip_text(&mut zip, "chapters/002-Chapter-1-Start.md");
    assert!(chapter_one.contains("# Chapter 1 Start"));
    assert!(chapter_one.contains("Section from the next spine still belongs to chapter one."));
    assert!(chapter_one.contains("## 1.1 Section"));
    assert!(!chapter_one.contains("Chapter 2 Next"));

    let chapter_two = read_zip_text(&mut zip, "chapters/003-Chapter-2-Next.md");
    assert!(chapter_two.contains("# Chapter 2 Next"));
    assert!(chapter_two.contains("<a id=\"deep\"></a>"));
    assert!(chapter_two.contains("Deep anchor target."));

    let readme = read_zip_text(&mut zip, "README.md");
    assert!(readme.contains("- [Chapter 1 Start](chapters/002-Chapter-1-Start.md)"));
    assert!(!readme.contains("1.1 Section"));
}

#[test]
fn preserves_single_spine_books_when_nav_entries_have_no_fragment_boundaries() {
    let temp = tempdir().unwrap();
    let epub = temp.path().join("single-spine-no-fragments.epub");
    make_single_spine_no_fragment_nav_epub(&epub);
    let output = temp.path().join("single-spine-no-fragments.zmd");

    convert_epub_to_zip(&epub, &output, false).unwrap();

    let mut zip = ZipArchive::new(File::open(&output).unwrap()).unwrap();
    let chapter_names = zip
        .file_names()
        .filter(|name| name.starts_with("chapters/") && name.ends_with(".md"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    assert_eq!(chapter_names, vec!["chapters/001-Chapter-1-Intro.md"]);

    let chapter = read_zip_text(&mut zip, "chapters/001-Chapter-1-Intro.md");
    assert!(chapter.contains("# Chapter 1 Intro"));
    assert!(chapter.contains("First chapter body."));
    assert!(chapter.contains("# Chapter 2 Parser"));
    assert!(chapter.contains("Second chapter body must not be deleted."));
    assert!(chapter.contains("```\nint main(void) {\n    return 0;\n}\n```"));
}

#[test]
fn missing_referenced_asset_fails_clearly() {
    let temp = tempdir().unwrap();
    let epub = temp.path().join("sample.epub");
    make_epub(&epub, false, false, true);
    let output = temp.path().join("sample.zip");

    let error = convert_epub_to_zip(&epub, &output, false).unwrap_err();
    match error {
        ConversionError::MalformedEpub(message) => assert!(message.contains("image asset")),
        other => panic!("unexpected error: {other:?}"),
    }
}

fn make_single_spine_no_fragment_nav_epub(path: &Path) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("META-INF/container.xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#).unwrap();

    zip.start_file("OEBPS/content.opf", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns:dc="http://purl.org/dc/elements/1.1/">
  <metadata><dc:title>Single Spine Book</dc:title></metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="body" href="body.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="body"/></spine>
</package>"#,
    )
    .unwrap();

    zip.start_file("OEBPS/toc.ncx", options).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
  <navMap>
    <navPoint id="c1" playOrder="1"><navLabel><text>Chapter 1 Intro</text></navLabel><content src="body.xhtml"/></navPoint>
    <navPoint id="c2" playOrder="2"><navLabel><text>Chapter 2 Parser</text></navLabel><content src="body.xhtml"/></navPoint>
  </navMap>
</ncx>"#).unwrap();

    zip.start_file("OEBPS/body.xhtml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<h1>Chapter 1 Intro</h1>
<p>First chapter body.</p>
<h1>Chapter 2 Parser</h1>
<p>Second chapter body must not be deleted.</p>
<pre><code>int main(void) {
    return 0;
}</code></pre>
</body></html>"#,
    )
    .unwrap();

    zip.finish().unwrap();
}

fn make_mixed_navigation_epub(path: &Path) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("META-INF/container.xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#).unwrap();

    zip.start_file("OEBPS/content.opf", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns:dc="http://purl.org/dc/elements/1.1/">
  <metadata><dc:title>Flat NCX Book</dc:title></metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>
    <item id="part1" href="part1.xhtml" media-type="application/xhtml+xml"/>
    <item id="part2" href="part2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="part1"/><itemref idref="part2"/></spine>
</package>"#,
    )
    .unwrap();

    zip.start_file("OEBPS/toc.ncx", options).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
  <navMap>
    <navPoint id="p" playOrder="1"><navLabel><text>Preface</text></navLabel><content src="part1.xhtml#preface"/></navPoint>
    <navPoint id="c1" playOrder="2"><navLabel><text>Chapter 1 Start</text></navLabel><content src="part1.xhtml#c1"/></navPoint>
    <navPoint id="s11" playOrder="3"><navLabel><text>1.1 Section</text></navLabel><content src="part2.xhtml#s11"/></navPoint>
    <navPoint id="c2" playOrder="4"><navLabel><text>Chapter 2 From NCX</text></navLabel><content src="part2.xhtml#c2"/></navPoint>
  </navMap>
</ncx>"#).unwrap();

    zip.start_file("OEBPS/nav.xhtml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="landmarks">
  <ol>
    <li><a href="part1.xhtml#preface">Cover Landmark</a></li>
  </ol>
</nav>
<nav epub:type="toc">
  <ol>
    <li><a href="part1.xhtml#preface">Preface</a></li>
    <li><a href="part1.xhtml#c1">Chapter 1 Start</a></li>
    <li><a href="part2.xhtml#s11">1.1 Section</a></li>
    <li><a href="part2.xhtml#c2">Chapter 2 Next</a></li>
  </ol>
</nav>
</body></html>"#,
    )
    .unwrap();

    zip.start_file("OEBPS/part1.xhtml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<p><span id="preface"></span>Preface</p>
<p><a href="part1.xhtml#c1">Chapter 1</a></p>
<p><a href="part2.xhtml#c2">Chapter 2</a></p>
<p><a href="part2.xhtml#deep">Deep target</a></p>
<p><span id="c1"></span>Chapter 1 Start</p>
<p>Opening content.</p>
</body></html>"#,
    )
    .unwrap();

    zip.start_file("OEBPS/part2.xhtml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<p><span id="s11"></span>1.1 Section</p>
<p>Section from the next spine still belongs to chapter one.</p>
<p><span id="c2"></span>Chapter 2 Next</p>
<p>Second chapter.</p>
<p><span id="deep"></span>Deep anchor target.</p>
</body></html>"#,
    )
    .unwrap();

    zip.finish().unwrap();
}

fn make_epub(path: &Path, declare_asset: bool, include_asset: bool, parent_ref: bool) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("META-INF/container.xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#).unwrap();

    let item = if declare_asset {
        if parent_ref {
            r#"<item id="img" href="media/pic.png" media-type="image/png"/>"#
        } else {
            r#"<item id="img" href="images/pic.png" media-type="image/png"/>"#
        }
    } else {
        ""
    };
    zip.start_file("OEBPS/content.opf", options).unwrap();
    write!(zip, r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns:dc="http://purl.org/dc/elements/1.1/">
  <metadata><dc:title>Sample Book</dc:title><dc:creator>Jane Author</dc:creator><dc:language>en</dc:language></metadata>
  <manifest>
    <item id="chap1" href="text/chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap2" href="text/chapter2.xhtml" media-type="application/xhtml+xml"/>
    {item}
  </manifest>
  <spine><itemref idref="chap1"/><itemref idref="chap2"/></spine>
</package>"#).unwrap();

    let src = if parent_ref {
        "../media/pic.png"
    } else {
        "../images/pic.png"
    };
    zip.start_file("OEBPS/text/chapter1.xhtml", options)
        .unwrap();
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<h1>Opening Chapter</h1><p>Opening Chapter</p>
<p>Hello <strong>reader</strong>. <a href="chapter2.xhtml#next">Second</a>.</p>
<p><img src="{src}" alt="Picture"/></p>
</body></html>"#
    )
    .unwrap();

    zip.start_file("OEBPS/text/chapter2.xhtml", options)
        .unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="next">Second Chapter</h1><p>Second.</p></body></html>"#).unwrap();

    if include_asset {
        let asset = if parent_ref {
            "OEBPS/media/pic.png"
        } else {
            "OEBPS/images/pic.png"
        };
        zip.start_file(asset, options).unwrap();
        zip.write_all(&[0x89, 0x50, 0x4e, 0x47]).unwrap();
    }
    zip.finish().unwrap();
}

fn read_zip_text(zip: &mut ZipArchive<File>, path: &str) -> String {
    let mut entry = zip.by_name(path).unwrap();
    let mut text = String::new();
    entry.read_to_string(&mut text).unwrap();
    text
}
