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
all2markdown ./corpus --metadata-only > inventory.jsonl  # metadata alone, no body parsed
find ./corpus -name '*.docx' | all2markdown --paths-from - -o ./out  # a list of paths, recursion is find's
curl -s https://store/report.docx | all2markdown - --name report.docx > out.md  # a document on stdin
```

Exit codes: 0 every document extracted; 1 the command failed or the one
document asked for could not be read; 2 the arguments were refused; 3 the
batch ran to its end and some of its documents failed.

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
  the two cannot drift. The raw metadata bag goes to JSONL only. An
  `Inventory` renders to the same JSONL line as an `Extraction`, `text` null.
- `Registry::inventory` is the Inventory path: same Envelope stripping, same
  detection, then the Parser's `metadata` and never its `extract`. The Batch
  in `batch.rs` is one function generic over what it does per document, so
  `inventory_paths` and `extract_paths` share threads, queue, cap and panic
  guard. `--metadata-only` in the CLI; it refuses `-o`.
- `template.rs` is the Output Template: `{name} {stem} {ext} {parent} {format}`,
  parsed once at startup so an unknown variable is refused before any document
  is read. The default `{name}.md` keeps the source extension, which is what
  makes `report.doc` and `report.pdf` unable to overwrite one another.
- The CLI requires `-o <DIR>` for a directory or a path list unless `--jsonl`;
  a single document goes to stdout. `-o` may never be the source directory.
- The CLI's `Input` is decided by the arguments alone, never by peeking at
  stdin: `-` is a document on stdin, `--paths-from FILE` (`-` for stdin) is a
  list of paths, one per line, streamed into the Batch as it is read. A stdin
  document is the one thing that bypasses `batch.rs`: it is read whole under
  the same size cap, then handed to `extract`/`inventory`. `--name` gives it
  the name detection and the front matter need; `-o` is refused for it.
- Progress is one line on stderr, redrawn in place, only when stderr is a
  terminal; warnings and per-document errors go to stderr in every mode, so
  piped stdout carries nothing but Markdown or JSONL.
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
