"""The Python seam: what no Rust test can see from the other side of the binding.

Five tests, on purpose. Detection, parsers, envelopes and rendering are the
core's to test; the CLI's flags are the CLI's. What only this seam can catch
is that the batch stays lazy, that the GIL is really released, and that the
S3 example the repository promises actually runs.
"""

import json
import os
import runpy
import threading
import time
from pathlib import Path

import pytest

import all2markdown

REPO = Path(__file__).resolve().parents[4]
FIXTURES = REPO / "tests" / "fixtures"
EXAMPLE = REPO / "examples" / "s3_batch.py"
REFERENCE = ["1000.doc", "1000.docx", "1000.pdf", "1000.rtf"]
# A sentence every reference fixture carries; losing it is losing text.
SENTENCE = "je mange du chocolat"


def fixture(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def test_bytes_in_extraction_out_and_a_failure_raised():
    extraction = all2markdown.extract_bytes(fixture("1000.docx"), "report.docx")

    assert extraction.format == "docx"
    assert extraction.name == "report.docx"
    assert extraction.error is None
    assert SENTENCE in extraction.markdown
    assert extraction.file_metadata == {"name": "report.docx", "size": 7402, "modified": None}
    assert extraction.document_metadata is not None
    assert extraction.to_markdown().startswith("---\nformat: \"docx\"\n")
    assert json.loads(extraction.to_jsonl())["format"] == "docx"

    # The path call is the same thing with the read done for you.
    from_path = all2markdown.extract(FIXTURES / "1000.docx")
    assert from_path.markdown == extraction.markdown
    assert from_path.file_metadata["modified"] is not None

    # One document, so a Failure has nothing else to be local to: it is raised.
    with pytest.raises(all2markdown.Failure, match="docx parser failed"):
        all2markdown.extract_bytes(b"not a zip at all" * 8, "broken.docx")


def test_extract_many_is_lazy():
    # A generator that counts how far it has been drawn. A batch that
    # collected its input, or its results, before yielding would have pulled
    # all of it by the time the first result comes back.
    pulled = 0

    def documents():
        nonlocal pulled
        for _ in range(200):
            pulled += 1
            yield ("1000.rtf", fixture("1000.rtf"))

    results = all2markdown.extract_many(documents(), workers=1)
    first = next(results)

    assert first.error is None
    assert pulled < 200, f"the input was drained ({pulled} items) before the first result"

    # Whatever is pulled ahead is bounded: waiting does not grow it. One
    # worker, a small queue in front and behind, and one document in each
    # hand; twenty is far more than that and far less than the corpus.
    time.sleep(0.2)
    assert pulled <= 20, f"{pulled} items pulled ahead of a consumer that reads nothing"

    assert sum(1 for _ in results) == 199, "everything else arrives once it is asked for"
    assert pulled == 200


def test_the_gil_is_released_during_extraction():
    # A Python thread ticks once a millisecond while the main thread extracts.
    # With the GIL held for the whole call, no tick can land strictly inside
    # the call; with it released, hundreds do. The document is sized on the
    # machine so that the call lasts long enough to be unambiguous.
    def ticks_during(work):
        ticks, stop = [], threading.Event()

        def tick():
            while not stop.is_set():
                ticks.append(time.monotonic())
                time.sleep(0.001)

        thread = threading.Thread(target=tick, daemon=True)
        thread.start()
        time.sleep(0.05)
        started = time.monotonic()
        work()
        ended = time.monotonic()
        stop.set()
        thread.join()
        margin = 0.05
        inside = sum(1 for t in ticks if started + margin < t < ended - margin)
        return inside, ended - started

    sentence = "Alors voila, je mange du chocolat ; café, crème brûlée. ".encode("cp1252")
    document = sentence * (4 * 1024 * 1024 // len(sentence))
    while True:
        started = time.monotonic()
        all2markdown.extract_bytes(document, "big.txt")
        if time.monotonic() - started >= 0.4 or len(document) >= 256 * 1024 * 1024:
            break
        document *= 2

    inside, elapsed = ticks_during(lambda: all2markdown.extract_bytes(document, "big.txt"))
    assert elapsed >= 0.3, "the document is too small for the test to mean anything"
    assert inside > 50, f"only {inside} ticks in {elapsed:.2f}s: the GIL was held by extract_bytes"

    results = all2markdown.extract_many([("big.txt", document)], workers=1)
    inside, elapsed = ticks_during(lambda: next(results))
    assert elapsed >= 0.3
    assert inside > 50, f"only {inside} ticks in {elapsed:.2f}s: the GIL was held by extract_many"


def test_a_failure_is_one_item_and_the_batch_goes_on(tmp_path):
    # Every way of handing a document over, in one list, with two that cannot
    # be read: bytes that are no format, and a path that is not there.
    missing = tmp_path / "missing.docx"
    items = [
        ("1000.pdf", fixture("1000.pdf")),
        str(FIXTURES / "1000.doc"),
        FIXTURES / "1000.rtf",
        fixture("1000.docx"),
        ("broken.docx", b"not a zip at all" * 8),
        missing,
        bytearray(fixture("1000.rtf")),
    ]

    results = list(all2markdown.extract_many(items, workers=2))

    assert len(results) == len(items), "one result per item, whatever happened to it"
    by_name = {r.name: r for r in results}
    assert by_name["broken.docx"].error is not None
    assert by_name["broken.docx"].markdown is None
    assert by_name[str(missing)].error is not None
    for name in ("1000.pdf", str(FIXTURES / "1000.doc"), str(FIXTURES / "1000.rtf")):
        assert by_name[name].error is None, by_name[name].error
        assert SENTENCE in by_name[name].markdown
    unnamed = [r for r in results if r.name is None]
    assert sorted(r.format for r in unnamed) == ["docx", "rtf"], "bytes alone are detected by signature"
    assert json.loads(by_name["broken.docx"].to_jsonl())["text"] is None

    # A fault in the iterable itself is the caller's, and is raised — after
    # the documents already pulled have been yielded, not instead of them.
    def documents():
        yield ("1000.rtf", fixture("1000.rtf"))
        raise ConnectionError("the store went away")

    results = all2markdown.extract_many(documents(), workers=1)
    assert next(results).error is None
    with pytest.raises(ConnectionError, match="the store went away"):
        next(results)


def test_the_s3_example_runs_unmodified(tmp_path, monkeypatch, capsys):
    boto3 = pytest.importorskip("boto3")
    moto = pytest.importorskip("moto")

    # A local object store, and the example's own bucket and prefix in it:
    # the reference corpus plus one object nothing can read.
    for variable in ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"):
        monkeypatch.setenv(variable, "testing")
    monkeypatch.setenv("AWS_DEFAULT_REGION", "us-east-1")
    monkeypatch.chdir(tmp_path)

    with moto.mock_aws():
        s3 = boto3.client("s3")
        s3.create_bucket(Bucket="my-bucket")
        for name in REFERENCE:
            s3.put_object(Bucket="my-bucket", Key=f"documents/{name}", Body=fixture(name))
        s3.put_object(Bucket="my-bucket", Key="documents/broken.docx", Body=b"not a zip" * 8)
        s3.put_object(Bucket="my-bucket", Key="elsewhere/1000.rtf", Body=fixture("1000.rtf"))

        runpy.run_path(str(EXAMPLE), run_name="__main__")

    printed = capsys.readouterr().out
    assert "FAILED  documents/broken.docx" in printed
    assert printed.rstrip().endswith("4 extracted, 0 suspect, 1 failed")

    lines = [json.loads(line) for line in (tmp_path / "output.jsonl").read_text("utf-8").splitlines()]
    assert sorted(line["name"] for line in lines) == [f"documents/{name}" for name in REFERENCE]
    for line in lines:
        assert SENTENCE in line["markdown"], line["name"]
        assert line["format"] == Path(line["name"]).suffix[1:]
