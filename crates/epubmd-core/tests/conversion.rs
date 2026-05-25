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
