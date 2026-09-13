//! Two renderings of one Extraction: Markdown with a YAML header, and a JSON
//! line. One renderer feeds both, so what one says the other says too.

use all2markdown_core::{
    extract, to_jsonl, to_markdown, BatchItem, Extraction, FrontMatter, Options, SourceDocument,
};
use std::collections::BTreeMap;
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("Cannot read fixture {}: {e}", path.display()))
}

fn extract_fixture(name: &str) -> Extraction {
    let data = fixture(name);
    extract(SourceDocument::named(name, &data), &Options::default())
        .unwrap_or_else(|e| panic!("extracting {name} failed: {e}"))
}

/// The YAML header of a rendered document, and the body after it.
fn split_front_matter(markdown: &str) -> (&str, &str) {
    let rest = markdown
        .strip_prefix("---\n")
        .expect("the document opens with a front matter fence");
    let end = rest
        .find("\n---\n")
        .expect("the front matter closes with a fence");
    // One blank line separates the closing fence from the text.
    let body = rest[end + 5..]
        .strip_prefix('\n')
        .unwrap_or(&rest[end + 5..]);
    (&rest[..end + 1], body)
}

/// The header as nested maps, each scalar kept as the JSON value it is
/// written as. Enough of a YAML reader for a header this tool writes: two
/// levels, one key per line, scalars that are JSON literals.
fn parse_header(header: &str) -> BTreeMap<String, serde_json::Value> {
    let mut top = BTreeMap::new();
    let mut section: Option<String> = None;
    for line in header.lines() {
        let indented = line.starts_with("  ");
        let (key, value) = line.trim().split_once(':').expect("key: value");
        let value = value.trim();
        if indented {
            let section = section
                .as_ref()
                .expect("an indented line belongs to a section");
            let parsed: serde_json::Value =
                serde_json::from_str(value).unwrap_or_else(|_| panic!("{value} is not JSON"));
            top.entry(section.clone())
                .or_insert_with(|| serde_json::Value::Object(Default::default()))
                .as_object_mut()
                .unwrap()
                .insert(key.to_owned(), parsed);
        } else if value.is_empty() {
            section = Some(key.to_owned());
        } else {
            section = None;
            let parsed: serde_json::Value =
                serde_json::from_str(value).unwrap_or_else(|_| panic!("{value} is not JSON"));
            top.insert(key.to_owned(), parsed);
        }
    }
    top
}

#[test]
fn the_front_matter_carries_the_format_the_encoding_and_the_metadata_core() {
    let extraction = extract_fixture("1000.docx");
    let rendered = to_markdown(&extraction, FrontMatter::On);
    let (header, body) = split_front_matter(&rendered);
    let header = parse_header(header);

    assert_eq!(header["format"], "docx");
    assert_eq!(header["encoding"], serde_json::Value::Null);
    assert_eq!(header["file"]["name"], "1000.docx");
    assert_eq!(header["file"]["size"], 7402);
    assert_eq!(header["document"]["language"], "fr-FR");
    assert_eq!(header["document"]["page_count"], 1);
    assert_eq!(header["document"]["created"], 1_693_257_950);
    assert_eq!(header["warnings"], serde_json::json!([]));
    assert!(body.contains("je mange du chocolat"));
}

#[test]
fn the_front_matter_leaves_the_raw_bag_to_jsonl() {
    let extraction = extract_fixture("1000.docx");
    assert!(
        !extraction.document.raw.is_empty(),
        "the fixture must declare something for this test to mean anything"
    );
    let rendered = to_markdown(&extraction, FrontMatter::On);
    let (header, _) = split_front_matter(&rendered);

    assert!(
        !header.contains("raw") && !header.contains("Application"),
        "the raw bag has no place in a human-read header:\n{header}"
    );

    let item = BatchItem {
        source: "1000.docx".to_owned(),
        result: Ok(extraction),
    };
    let line: serde_json::Value = serde_json::from_str(&to_jsonl(&item)).unwrap();
    assert_eq!(line["document"]["raw"]["revision"], "15");
}

#[test]
fn the_front_matter_is_present_even_when_the_metadata_is_empty() {
    // doc declares nothing; its header still carries every key, null.
    let extraction = extract_fixture("1000.doc");
    assert!(extraction.document.is_empty());
    let rendered = to_markdown(&extraction, FrontMatter::On);
    let (header, _) = split_front_matter(&rendered);
    let header = parse_header(header);

    assert_eq!(header["format"], "doc");
    for key in [
        "title",
        "author",
        "created",
        "modified",
        "page_count",
        "language",
    ] {
        assert_eq!(
            header["document"][key],
            serde_json::Value::Null,
            "{key} is present and null"
        );
    }
}

#[test]
fn every_front_matter_has_the_same_shape_whatever_the_document_declared() {
    let shape = |name: &str| {
        let rendered = to_markdown(&extract_fixture(name), FrontMatter::On);
        let (header, _) = split_front_matter(&rendered);
        header
            .lines()
            .map(|line| line.split(':').next().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let mut shapes: Vec<Vec<String>> = ["1000.doc", "1000.docx", "1000.rtf", "1000.pdf"]
        .iter()
        .map(|name| shape(name))
        .collect();
    shapes.dedup();
    assert_eq!(shapes.len(), 1, "one shape for every document: {shapes:?}");
}

#[test]
fn the_front_matter_can_be_switched_off() {
    let extraction = extract_fixture("1000.docx");
    let rendered = to_markdown(&extraction, FrontMatter::Off);
    assert_eq!(rendered, extraction.markdown);
    assert!(!rendered.starts_with("---"));
}

#[test]
fn a_declared_value_cannot_break_the_header() {
    // A title with a colon, a newline and a quote: each would derail a YAML
    // reader if written bare. Written as a JSON string, none of them can.
    let mut extraction = extract_fixture("1000.docx");
    extraction.document.title = Some("a: \"b\"\n---\nc".to_owned());
    let rendered = to_markdown(&extraction, FrontMatter::On);
    let (header, _) = split_front_matter(&rendered);
    let header = parse_header(header);
    assert_eq!(header["document"]["title"], "a: \"b\"\n---\nc");
}

#[test]
fn markdown_and_jsonl_say_the_same_thing() {
    // The proof that one renderer feeds both: every value the header carries
    // is the value the JSON line carries, for every fixture.
    for name in ["1000.doc", "1000.docx", "1000.rtf", "1000.pdf"] {
        let extraction = extract_fixture(name);
        let rendered = to_markdown(&extraction, FrontMatter::On);
        let (header, body) = split_front_matter(&rendered);
        let header = parse_header(header);

        let item = BatchItem {
            source: name.to_owned(),
            result: Ok(extraction),
        };
        let line: serde_json::Value = serde_json::from_str(&to_jsonl(&item)).unwrap();

        assert_eq!(header["format"], line["format"], "{name}");
        assert_eq!(header["encoding"], line["encoding"], "{name}");
        assert_eq!(header["file"], line["file"], "{name}");
        assert_eq!(header["warnings"], line["warnings"], "{name}");
        for (key, value) in header["document"].as_object().unwrap() {
            assert_eq!(value, &line["document"][key], "{name}: document.{key}");
        }
        assert_eq!(body, line["text"].as_str().unwrap(), "{name}");
    }
}
