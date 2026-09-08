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

- `FormatParser` trait in `strategy.rs`, one impl per format
- Magic-byte detection in `detect.rs`, dispatch via a `match` in `lib.rs`.
  Milestone 1 step 2 replaces both with a Parser registry, so that adding a
  format touches one file instead of three.
- DOCX parser skips `RunChild::Drawing` and `RunChild::Shape` to exclude
  textbox text
- DOC uses `unword`, DOCX uses `docx-rs`, RTF uses `rtf-parser`, PDF uses
  `pdf-extract`

## Key Conventions

- All parsers take `&[u8]` and return `Result<String, Error>`
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
