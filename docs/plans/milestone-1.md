# Milestone 1 — the full plumbing on the four existing formats

Goal: a published v0.1, usable end to end, where **adding a format means
creating one file and adding one line**. No new format is added here: doc, docx,
rtf and pdf only.

Rationale: the project's requirements (parallelism, installability,
extensibility) live in the plumbing, not in the parsers. Covering 18 formats
before the trait is stable would mean rewriting all of them.

## Settled decisions

| Topic | Decision |
|---|---|
| Contract | Text-first extraction, structure best-effort (ADR-0001) |
| Scope | Circles 1 and 2; no OCR, no image, no audio, no video |
| Detection | Content > extension > heuristic > fall back to `txt` if decodable, else Failure; `--strict` disables the fallback |
| Encoding | Guessed (BOM, UTF-8 validation, statistics, `cp1252` fallback), overridable |
| Envelopes | gzip, zstd, xz, bzip2; depth 1; one document per envelope |
| Size cap | 500 MB on the **decompressed** size, checked during decompression; exceeding it is a Failure |
| Result | An object, never a string: markdown, format, encoding, metadata, warnings |
| Metadata | Two separate blocks (file / document), normalised core plus raw bag; front matter on by default, `--no-front-matter`; `--metadata-only` emits JSONL |
| Failure | Always local to one document, never interrupts the Batch, never panics |
| Parallelism | rayon on the Rust side, GIL released; `workers=` / `-j`, default = core count |
| Ordering | Completion order by default, `--ordered` for input order |
| Network I/O | None (ADR-0002) |
| Output | `-o` required, template variables `{name} {stem} {ext} {parent} {format}`; JSONL as an alternative |
| CLI input | Current directory without recursion, path list on stdin, document on stdin |
| Packaging | abi3-py310 wheels, all platforms plus musllinux, zero dependency (ADR-0003) |
| Naming | `all2markdown` everywhere: repo, crates, binary (alias `a2md`), PyPI package |
| MSRV | Recent stable Rust; the 1.88 note in the git history is obsolete |
| PDF | Pure Rust, quality cap accepted and measured; reopened at milestone 3 |

## Steps

### 0. Rename and hygiene

_Done._

Repo, crate directories, `all2markdown-core` / `-cli` / `-python`, binary
`all2markdown` plus the `a2md` alias, Python module `all2markdown`. Add a
`.gitignore` (`target/`, `.~lock.*#`, the benchmark corpus).

*Check*: `cargo build` passes, no occurrence of `all2md` outside the ADRs.

### 1. The model

_Done._

The glossary types: `SourceDocument`, `Extraction`, `Warning`, `Failure`,
`FileMetadata`, `DocumentMetadata`, `Confidence`, `Format`. No logic, only data.
This is where the stability of everything else is decided.

*Check*: `examples/s3_batch.py` describes exactly the fields exposed; any
divergence is a design bug to fix now.

### 2. The Parser trait and the registry

_Done._

Three capabilities: recognise itself (`probe` -> `Confidence`), return its
metadata (default implementation: empty), extract the text. The registry holds
the Parsers and polls them all, keeping the most confident.

Intended consequence: `detect.rs` and the `match` in `lib.rs` **disappear**. The
`txt` fallback stops being a special case: it is simply the only Parser that
answers "last resort".

*Check*: adding a dummy Parser touches one file plus one registration line.

### 3. Detection and encoding

_Done._

Assembling the cascade and the encoding detection, plus the last-resort `txt`
Parser and its decodability test (encoding identified, no null bytes, low
control-character ratio). The `--strict` flag.

*Check*: a file with no extension and no signature, in cp1252, comes out as
`txt` with the detected encoding and a warning.

### 4. Envelopes

_Done._

Stripping gzip/zstd/xz/bzip2 before detection, depth 1, size cap applied to the
decompressed size during decompression.

*Check*: `reference.doc.gz` yields the same output as `reference.doc`; a
decompression bomb yields a Failure, not an OOM.

### 5. Porting the four existing parsers

_Done._

doc, docx, rtf and pdf moved onto the new trait, with metadata extraction and
warning emission (notably: PDF with no text layer).

*Check*: the four reference fixtures pass the sentinel markers.

### 6. Rendering

_Done._

Two outputs from the same `Extraction`: a Markdown file with YAML front matter
(core metadata only), and a JSONL line (core plus raw bag). A single rendering
path, so the two can never diverge.

### 7. Parallel batch

`rayon`, an iterator of results, `--ordered`, worker count control, progress on
stderr when the output is a terminal.

*Check*: throughput grows with `-j`; `Ctrl-C` interrupts cleanly; a corrupt
document mid-batch interrupts no other.

### 8. CLI

Inputs (current directory, path list on stdin, document on stdin), `-o` with a
template, `--format`, `--strict`, `--metadata-only`, `--no-front-matter`,
`--jsonl`, `--ordered`, `-j`, `--max-size`.

### 9. Python binding

`extract`, `extract_bytes`, `extract_many` (accepts any iterable, returns an
iterator, releases the GIL), `workers=`. `.pyi` stubs shipped.

*Check*: `examples/s3_batch.py` runs unmodified against a local MinIO.

### 10. Packaging and CI

maturin, `pyproject.toml`, abi3-py310, CLI entry point in the wheel.
GitHub Actions: manylinux x86_64/aarch64, musllinux, macOS arm64/x86_64,
Windows x86_64; PyPI publication via trusted publishing on tag; `cargo publish`;
static binaries in releases.

*Check*: `pip install` in a `python:3.10-alpine` container with no compiler
yields both the library **and** the command.

### 11. Tests

Three test seams, and no more. A seam is a door a test knocks on; each one is a
contract frozen in place, so the fewest and highest possible win.

| Seam | The door | What only it can catch |
|---|---|---|
| Core | `all2markdown_core::extract` | Detection, encoding, envelopes, parsers, rendering, batching |
| CLI | `all2markdown_cli::run` | Flags, the `-o` template, stdin, exit codes |
| Python | `import all2markdown` | The iterator stays lazy, the GIL is released |

The Python seam stays deliberately small, around five tests. It exists because
an `extract_many` that quietly materialises its results would break the
million-document target, and no Rust test can observe that from the other side
of the binding.

- Reference corpus (`tests/fixtures/reference/`): one file per format, produced
  by hand from `reference.txt` with the styles applied. Assertions on the
  sentinel markers.
- Snapshots, using `--ordered` for determinism.
- Parser fuzzing: guarantee "never panics".
- Invariants over a govdocs1 sample: no panic, no hang, no OOM.

### 12. Benchmark

Corpus: ~10,000 govdocs1 files, outside the repo
(`/Linux_data/corpora/govdocs1`), downloaded by a versioned script; dependent
tests skip when it is absent.

**Speed dimension first**: throughput in docs/s, p50/p99 latency, peak RSS, cold
start time, install size, and above all the throughput curve at 1/4/8/16/24
workers — that curve is the project's thesis.

Competitors: `firecrawl-anydoc` and `tika-server` (a JVM: warm-up is mandatory,
otherwise the comparison is dishonest). The protocol is published in the repo so
it is reproducible and contestable.

The **quality dimension** (text recovery rate over the reference corpus) comes
right after: a speed-only benchmark misleads, since extracting less is always
faster.

## Out of scope for milestone 1

Circle 1 (milestone 2); the rest of circle 2 (milestone 3); reopening the PDF
backend; registering Parsers from Python; multi-document archives.
