# Reference corpus

`reference.txt` is the single source of truth for all2markdown's tests.

The same content is saved in every supported format, as `reference.<ext>`. The
tests assert that the text extracted from each format carries the same elements
as the source.

The prose is deliberately French: accented characters are the cheapest way to
catch an encoding failure.

## Verification markers

Every format must return these strings, without exception:

- `SENTINELLE-DEBUT-9F3A1C` (start of document, catches truncation at the head)
- `SENTINELLE-FIN-7B2E4D` (end of document, catches truncation at the tail)
- `FIN-PARA-LONG` (catches truncation of the long paragraph)
- `APRES-SAUT-DE-PAGE` (catches content lost after a page break)
- `NOTE-BAS-PAGE` (catches lost footnotes)
- The Unicode witness line in section 1 (catches an encoding failure)

## How to generate the fixtures

The bracketed markers (`[HEADING 1]`, `[TABLE 3 COLUMNS]`, `[BULLET LIST]`, ...)
describe the formatting to apply in the word processor. They must **not** appear
in the final document: they are instructions, not content.

1. Open `reference.txt` and copy its content into a blank document.
2. Apply the styles the markers call for, then delete the markers.
3. Save as `reference.odt`, `reference.docx`, `reference.doc`, `reference.rtf`,
   `reference.pdf`, `reference.html`.
4. For the final `reference.txt`: keep the content without the markers.
