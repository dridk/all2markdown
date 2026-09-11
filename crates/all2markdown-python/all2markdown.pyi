"""Extracts text from any text-bearing file and renders it as Markdown.

Bytes in, an Extraction out; a batch takes any iterable and returns an
iterator, with the GIL released for the whole of the extraction.
"""

from collections.abc import Iterable, Iterator
from os import PathLike
from typing import Any, TypeAlias

DEFAULT_MAX_SIZE: int
"""The default cap on a decompressed document, in bytes: 500 MB."""

BytesLike: TypeAlias = bytes | bytearray | memoryview
Item: TypeAlias = str | PathLike[str] | BytesLike | tuple[str | None, BytesLike]
"""One input to `extract_many`: a path, a document's bytes, or a (name, bytes) pair."""

class Failure(Exception):
    """One document could not be extracted.

    Raised by `extract` and `extract_bytes`, which have one document and
    nothing else for a failure to be local to. In `extract_many` the same
    Failure is the item's `error`, and the batch goes on.
    """

class Extraction:
    """What all2markdown produced for one document, or why it could not.

    One class for both outcomes, so a batch is one stream: `error` is None
    for a document that extracted and its message otherwise, in which case
    every other field but `name` is None.
    """

    @property
    def name(self) -> str | None:
        """The path the document was read from, or the name given with its bytes."""
    @property
    def format(self) -> str | None:
        """The id of the format the document was read as: "docx", "pdf", ..."""
    @property
    def encoding(self) -> str | None:
        """The encoding the text was decoded from, where that is a meaningful question."""
    @property
    def markdown(self) -> str | None:
        """The text, as Markdown, without front matter."""
    @property
    def warnings(self) -> list[str]:
        """Why the document is suspect, if it is."""
    @property
    def error(self) -> str | None:
        """Why there is no text, or None when there is."""
    @property
    def file_metadata(self) -> dict[str, Any] | None:
        """`name`, `size`, `modified` (a Unix timestamp, None for bytes)."""
    @property
    def document_metadata(self) -> dict[str, Any] | None:
        """`title`, `author`, `created`, `modified`, `page_count`, `language`,
        and under `raw` everything else the format exposed, verbatim."""
    def to_markdown(self, front_matter: bool = True) -> str:
        """The Markdown document, with the YAML front matter the command writes.

        Raises `Failure` for a document that did not extract.
        """
    def to_jsonl(self) -> str:
        """The JSON line the command writes for this document, without its newline."""

class Results(Iterator[Extraction]):
    """The batch of `extract_many`, as an iterator.

    Results arrive in completion order, as they finish. Dropping it before
    the end stops the work.
    """

    def __iter__(self) -> Results: ...
    def __next__(self) -> Extraction: ...

def extract(
    path: str | PathLike[str],
    format: str | None = None,
    *,
    encoding: str | None = None,
    strict: bool = False,
    max_size: int = DEFAULT_MAX_SIZE,
) -> Extraction:
    """Extract one document from its path. Raises `Failure` when it cannot be read.

    `format` forces the format rather than detecting it; `encoding` does the
    same for the text encoding; `strict` refuses a document identified by
    nothing but its being decodable; `max_size` caps the document,
    decompressed, in bytes.
    """

def extract_bytes(
    data: BytesLike,
    name: str | None = None,
    *,
    format: str | None = None,
    encoding: str | None = None,
    strict: bool = False,
    max_size: int = DEFAULT_MAX_SIZE,
) -> Extraction:
    """Extract one document from its bytes, which never touch the disk.

    `name` is what detection falls back on for the formats that carry no
    signature, and what the result is labelled with: give the key or the file
    name when there is one. Raises `Failure` when the bytes cannot be read.
    """

def extract_many(
    items: Iterable[Item],
    *,
    workers: int | None = None,
    format: str | None = None,
    encoding: str | None = None,
    strict: bool = False,
    max_size: int = DEFAULT_MAX_SIZE,
) -> Results:
    """Extract many documents at once, from any iterable, as an iterator.

    Documents are pulled from the iterable only as the workers need them, so
    a generator that downloads as it goes is the intended input, and results
    are yielded as they finish, in completion order. A document that cannot
    be read is an item with its `error` set, never an exception. An exception
    raised by the iterable itself is raised from the iterator, once the
    documents already pulled have been yielded.

    `workers` is the number of extraction threads; None or 0 means one per core.
    """
