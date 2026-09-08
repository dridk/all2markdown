# all2markdown

Extracts text from any text-bearing file and renders it as Markdown, at the
scale of a million documents.

- **One wheel, no dependencies.** `pip install all2markdown` works offline, in a
  `distroless` image, with every supported format included. No system library,
  no Python runtime dependency, no optional extras.
- **Built for batches.** Conversion runs in Rust with the GIL released, so a
  million documents saturate every core from a single process.
- **Text is never lost.** Formatting is dropped freely; text is not. Every
  extraction reports the format and encoding it detected, and warns when a
  document looks suspect.

Status: pre-release. See `docs/plans/milestone-1.md`.

## Documentation

- `CONTEXT.md` — the domain vocabulary this project speaks
- `docs/adr/` — the decisions that shaped the design, and why
- `examples/s3_batch.py` — bulk conversion from an object store
