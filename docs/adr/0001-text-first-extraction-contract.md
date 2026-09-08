# Text-first extraction contract

all2markdown targets automated pipelines processing millions of documents, not
human readers. We therefore extract the whole text and render structure
(headings, lists, tables) only where it is detectable, never guaranteeing
faithful formatting.

The resulting invariant governs the whole project: **text is never lost,
formatting is lost freely**. An extraction that drops text is a bug; an
extraction that flattens a table into paragraphs is not.

## Options considered

**Faithful conversion** (a readable equivalent of the original: bold, images,
footnotes, numbering). Rejected: it multiplies the code per format, the failure
modes and the processing time, for a quality that does not serve a machine
consumer.

## Consequences

- A Parser that returns plain text where a table stood is conformant.
- A Parser that truncates a paragraph breaks the contract, however pretty the
  output. The reference tests rest on sentinel markers that detect exactly this
  kind of loss.
- The word "conversion" is banned from the code and the documentation: it
  implies a contract we do not honour.
