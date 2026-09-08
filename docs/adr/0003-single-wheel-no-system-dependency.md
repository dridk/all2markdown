# One self-contained wheel rather than optional extras

`pip install all2markdown` must work everywhere, offline, including inside a
`distroless` image, and must give access to **every** supported format. We
therefore ship abi3 wheels (Python 3.10 minimum) containing everything, with no
Python runtime dependency and no C system library.

A format is admitted into scope only if it is readable in pure Rust: that
admission rule is the accepted price of the installation promise. This is why
OCR, scanned PDFs, images, audio and video are out of scope.

## Options considered

**The extras model** (`pip install all2markdown[pdf,ocr,docx]`), as used by the
homonymous PyPI package `all2md` with some forty extras. Rejected: it pushes
installation complexity onto the user, produces runtime failures ("why doesn't
my docx work?") and multiplies the combinations to test.

## Consequences

- Published platforms: manylinux x86_64/aarch64, musllinux, macOS
  arm64/x86_64, Windows x86_64. A missing platform breaks the promise.
- The CLI ships through three channels: an entry point in the wheel,
  `cargo install`, and static binaries in GitHub releases.
- PDF extraction quality is capped at what pure Rust can do. That cap is
  accepted, measured, and reopened at milestone 3 on evidence (a statically
  linked PDFium would remain compatible with this decision, at a CI cost).
