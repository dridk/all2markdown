# Write our own parsers rather than depend on firecrawl/anydoc

`firecrawl/anydoc` (Rust, MIT, Python/Node/WASM bindings) covers the same circle
of office formats as all2markdown, with the same content-based detection. We
chose to write our own parsers anyway.

A future reader will find this surprising: depending on anydoc would have
delivered circle 2 immediately, under a permissive licence, and the Parser
registry architecture would have accepted it as just another Parser.

## What anydoc does not cover

These gaps motivated all2markdown's design and remain its reason to exist:

- no batch or parallel API (the caller writes the loop)
- no circle 1 format other than CSV: no txt, md, json, xml, html, eml, mbox
- no compressed envelopes
- no announced encoding detection
- no traceability: no warnings, no per-document failure, no inventory mode, no
  exposed metadata

## Consequences

- The cost of the project is dominated by the parsers, not by the fleet layer.
- The decision is reversible **format by format**: the registry allows
  substituting a Parser that delegates to anydoc if our quality proves inferior
  on a given format.
- The benchmark compares all2markdown against anydoc and tika-server. An
  unfavourable quality gap on a format must reopen this ADR for that format.
