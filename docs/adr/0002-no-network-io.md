# all2markdown performs no network I/O

all2markdown turns bytes into Markdown; it does not know where the bytes live.
Downloading from S3, GCS, HTTP or any other remote store is the caller's job,
and the caller hands the bytes to the API.

This is surprising, because "convert an S3 bucket" is a common request. We
refuse it because an embedded S3 client drags in credentials, regions, retries,
pagination and token expiry — then GCS, then Azure, then SFTP. That is a product
in its own right grafted onto a text extractor, and a security surface we do not
want to maintain.

## Consequences

- The Python API exposes a bytes variant and accepts any Python iterable, so a
  generator can download as it goes.
- Overlap stays efficient: downloading blocks on the network and releases the
  GIL, extraction runs in Rust and releases it too.
- The CLI accepts a document on stdin, which covers the remote case from a
  shell.
- `examples/s3_batch.py` documents the recipe. `boto3` will never be a
  dependency.
