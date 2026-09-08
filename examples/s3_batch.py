"""Bulk conversion of documents stored on S3.

all2markdown performs no network I/O: it turns bytes into Markdown, it does not
know where the bytes live. Downloading is the caller's job, which lets you use
any client (boto3, s3fs, minio, an NFS mount, a database) without all2markdown
depending on it.

Why this stays fast despite going through Python:

  - downloading blocks on the network, so CPython releases the GIL;
  - extraction runs in Rust, and releases the GIL for its whole duration.

Both stages therefore make real progress in parallel, in a single process, with
no multiprocessing and no intermediate serialisation.
"""

import json
from concurrent.futures import ThreadPoolExecutor

import boto3

import all2markdown

BUCKET = "my-bucket"
PREFIX = "documents/"

DOWNLOADERS = 32   # network I/O threads
EXTRACTORS = 0     # Rust extraction threads; 0 means one per core
WINDOW = 1000      # maximum documents downloaded ahead

s3 = boto3.client("s3")


def list_keys(bucket: str, prefix: str):
    """Enumerate the bucket's keys page by page, without loading them all."""
    paginator = s3.get_paginator("list_objects_v2")
    for page in paginator.paginate(Bucket=bucket, Prefix=prefix):
        for obj in page.get("Contents", []):
            yield obj["Key"]


def download(key: str) -> tuple[str, bytes]:
    return key, s3.get_object(Bucket=BUCKET, Key=key)["Body"].read()


def in_chunks(iterable, size):
    chunk = []
    for item in iterable:
        chunk.append(item)
        if len(chunk) == size:
            yield chunk
            chunk = []
    if chunk:
        yield chunk


def source(keys):
    """Yield (name, bytes) pairs as they arrive.

    Chunking matters: submitting a million keys at once to ThreadPoolExecutor
    would create a million futures in memory. The window bounds memory to
    WINDOW documents downloaded ahead.
    """
    with ThreadPoolExecutor(DOWNLOADERS) as pool:
        for chunk in in_chunks(keys, WINDOW):
            yield from pool.map(download, chunk)


def main() -> None:
    extracted = suspect = failed = 0

    with open("output.jsonl", "w", encoding="utf-8") as out:
        extractions = all2markdown.extract_many(
            source(list_keys(BUCKET, PREFIX)),
            workers=EXTRACTORS,
        )

        # extract_many returns an iterator: results arrive as they are produced,
        # memory stays bounded, and Ctrl-C interrupts cleanly.
        for extraction in extractions:
            if extraction.error is not None:
                failed += 1
                print(f"FAILED  {extraction.name}: {extraction.error}")
                continue

            if extraction.warnings:
                suspect += 1
                print(f"SUSPECT {extraction.name}: {', '.join(extraction.warnings)}")

            extracted += 1
            out.write(
                json.dumps(
                    {
                        "name": extraction.name,
                        "format": extraction.format,
                        "encoding": extraction.encoding,
                        "document": extraction.document_metadata,
                        "markdown": extraction.markdown,
                        "warnings": extraction.warnings,
                    },
                    ensure_ascii=False,
                )
                + "\n"
            )

    print(f"\n{extracted} extracted, {suspect} suspect, {failed} failed")


if __name__ == "__main__":
    main()
