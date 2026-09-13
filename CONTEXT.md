# all2markdown

Extracts the text from any file that carries some and renders it as Markdown, at
the scale of a million documents.

## The document

**Source Document** (`SourceDocument`):
Any text-bearing file submitted to all2markdown.
_Avoid_: text file, input, blob, file

**Envelope** (`Envelope`):
A compression layer wrapping a Source Document, stripped before any detection.
It holds exactly one Source Document; multi-document archives are out of scope.
_Avoid_: archive, container, wrapper

**Supported Format** (`Format`):
A format all2markdown can read. A format is admitted only if it is readable in
pure Rust, with no system dependency.
_Avoid_: file type, mime type, extension

## Extracting

**Extract** (`extract`):
To recover the whole text of a Source Document and render its structure where
that structure is detectable. Formatting is not rendered.
_Avoid_: convert, transform, parse

**Extraction Invariant**:
Text is never lost; formatting is lost freely. An extraction that drops text is
a bug; an extraction that flattens a table into paragraphs is not.

**Parser** (`Parser`):
The unit of extension: an object that knows one Supported Format, can recognise
itself in a stream of bytes, and can extract text from it. Adding a format means
adding a Parser.
_Avoid_: converter, handler, backend, strategy, driver

**Confidence** (`Confidence`):
A Parser's answer when offered a Source Document: certain, likely, last resort,
or no. The most confident Parser wins.
_Avoid_: score, priority, weight

**Forced Format** (`ForcedFormat`):
A format imposed by the caller, which bypasses detection and always wins.
_Avoid_: override, hint

## The result

**Extraction** (`Extraction`):
What all2markdown produces for one Source Document: the Markdown, the Supported
Format chosen, the detected encoding, the metadata and the Warnings. Never a
bare string.
_Avoid_: output, result, document

**Inventory** (`Inventory`):
What all2markdown produces for one Source Document when only its metadata is
asked for: the Supported Format, the File Metadata and the Document Metadata.
The body is never parsed, which is what makes an Inventory of a corpus an
order of magnitude faster than its Extraction.
_Avoid_: listing, index, scan, metadata-only result

**Warning** (`Warning`):
Marks an Extraction that succeeded but is suspect, typically a non-empty
document that yielded zero characters.
_Avoid_: note, remark

**Failure** (`Failure`):
Marks the absence of any Extraction for a Source Document. A Failure is always
local: it never interrupts the Batch.
_Avoid_: fatal error, crash, exception

**File Metadata** (`FileMetadata`):
What the filesystem or object store knows about the Source Document: name, size,
modification time. Always available.
_Avoid_: metadata

**Document Metadata** (`DocumentMetadata`):
What the Source Document declares about itself: title, author, creation date,
page count. Absent from formats that carry none.
_Avoid_: metadata, properties

## Working at scale

**Batch** (`Batch`):
A set of Source Documents processed in a single call. This is the nominal mode
of use: the reference scale is a million documents.
_Avoid_: job, queue, run

**Output Template** (`OutputTemplate`):
The model naming the file produced from a Source Document, by variable
substitution. One input yields one file; there is no pattern matching.
_Avoid_: pattern, mask, rename rule
