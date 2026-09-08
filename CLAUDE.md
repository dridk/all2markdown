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
all2markdown -i file.docx > out.md            # auto-detect format
all2markdown -i file.doc -f doc > out.md      # explicit format
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
- DOCX parser skips `RunChild::Drawing` and `RunChild::Shape` to exclude
  textbox text
- DOC uses `unword`, DOCX uses `docx-rs`, RTF uses `rtf-parser`, PDF uses
  `pdf-extract`

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
