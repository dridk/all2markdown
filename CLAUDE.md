# all2markdown

Extracts text from any text-bearing file and renders it as Markdown, at the
scale of a million documents. Rust core, with a CLI and Python bindings.

Read `CONTEXT.md` for the domain vocabulary and `docs/adr/` for the decisions
that shaped the design. `docs/plans/milestone-1.md` is the work in progress.

## Structure

Cargo workspace with 3 crates:
- `crates/all2markdown-core` - library (format detection, parsers)
- `crates/all2markdown-cli` - binary (`all2markdown`, alias `a2md`)
- `crates/all2markdown-python` - PyO3 bindings (maturin), imports as `all2markdown`

## Build

```bash
cargo build --release                            # CLI binary
cd crates/all2markdown-python && maturin develop  # Python wheel (dev)
```

## Test

```bash
cargo test
cd crates/all2markdown-python && maturin develop && python -m pytest python/tests/
```

## CLI Usage

```bash
all2markdown -i file.docx > out.md            # auto-detect format, front matter on
all2markdown -i file.doc -f doc > out.md      # explicit format
all2markdown ./corpus -o ./out -j 8           # a directory, one .md per document
all2markdown ./corpus -o ./out -t '{stem}-{format}.md' --no-front-matter
all2markdown ./corpus -j 8 --jsonl > out.jsonl  # a directory, as JSONL on stdout
```

## Architecture

Current state, being reshaped by milestone 1:

- `Parser` trait in `parser.rs`, one impl per format under `parsers/`
- `Registry` in `registry.rs` holds the Parsers, polls every one of them for a
  `Confidence` and keeps the highest, ties broken by registration order. It also
  owns the id-to-Format lookup, since only it can know about a Parser registered
  by a consumer of the crate.
- Adding a format means one new file in `parsers/` plus its `mod` and
  `register` lines in `parsers/mod.rs`; nothing else changes. A Parser written
  outside this crate costs one `Registry::register` call and no edit here.
- Detection is the Confidence ordering and nothing else: a Parser answers
  `Certain` on a signature, `Likely` on an extension, `LastResort` on mere
  decodability. The `txt` Parser is that last rung, registered like any other,
  which is why `--strict` is one line — it drops `LastResort`.
- `envelope.rs` strips gzip/zstd/xz/bzip2 before detection, one deep, capped on
  the *decompressed* size as it inflates
- `batch.rs` is the parallel path: rayon over a bounded queue, results in
  completion order, each document's panic caught so it costs one document
- `render.rs` is the one rendering path: a single `Provenance` struct feeds
  both the YAML front matter of `to_markdown` and the line of `to_jsonl`, so
  the two cannot drift. The raw metadata bag goes to JSONL only.
- `template.rs` is the Output Template: `{name} {stem} {ext} {parent} {format}`,
  parsed once at startup so an unknown variable is refused before any document
  is read. The default `{name}.md` keeps the source extension, which is what
  makes `report.doc` and `report.pdf` unable to overwrite one another.
- The CLI requires `-o <DIR>` for a directory unless `--jsonl`; a single
  document goes to stdout. `-o` may never be the source directory.
- DOCX parser skips `RunChild::Drawing` and `RunChild::Shape` to exclude
  textbox text
- DOC uses `unword`, DOCX uses `docx-rs`, RTF uses `rtf-parser`, PDF uses
  `pdf-extract`
- Document Metadata comes only from what those libraries expose: DOCX through
  its `docProps` parts, PDF through the Info dictionary. `unword` and
  `rtf-parser` expose none, so doc and rtf declare none.

## Key Conventions

- A Parser takes a `&SourceDocument` and returns `Result<String, Failure>`
- Output is Markdown with `#` headings where detectable, plain text otherwise
- Say "extract", never "convert": the contract is text-first, not faithful
  conversion (ADR-0001)

## Agent skills

### Issue tracker

Issues live as GitHub issues on the repo's `origin` remote, driven by the `gh`
CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, each label string equal to its name: `needs-triage`,
`needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` at the root, ADRs in `docs/adr/`. See
`docs/agents/domain.md`.
