# proof

A fast, streaming validator for Word documents (.docx) that gives the same
verdict as Microsoft's [Open XML SDK](https://github.com/dotnet/Open-XML-SDK)
validator (`OpenXmlValidator(FileFormatVersions.Microsoft365)`, SDK 3.5.1):
schema, markup compatibility, semantic constraints and package structure.
No .NET, no DOM: one pass over each part.

```
cargo build --release
./target/release/proof file.docx another.docx
{"path":"file.docx","ok":true,"ids":[],"parts":[],"first":""}
```

On a 166-page document (6.9 MB `document.xml`): 0.2 s and 19 MB, where the SDK
takes 1.9 s and 190 MB.

## How it stays faithful

- `gen/` turns the SDK's own schema data, part definitions and generated
  constraint registrations into static tables (`src/schema_gen.rs`), resolved
  for Microsoft365. Run `cargo run --release -p gen -- PATH/TO/Open-XML-SDK
  src/schema_gen.rs` against a checkout of the SDK at v3.5.1.
- The validator ports the SDK's algorithms as they run, quirks included
  (DESIGN.md lists the ones that matter).
- `fuzz/` checks it against the SDK itself: `fuzz/oracle` is a small .NET
  program around the SDK; `fuzz/diff.py` compares verdicts and error ids on a
  list of files; `fuzz/mutate.py` makes mutants of real documents to compare
  on. The bar: proof never accepts a document the SDK rejects.

## Status

Every reachable part of a WordprocessingML package. Error ids match the SDK;
descriptions are proof's own, and errors carry no XPath yet.
