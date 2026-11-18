# Changelog

## npm 0.2.1, crate 0.3.1

Faster, and one fix.

- A sheet's `codeName` is checked against the SDK's pattern. The pattern used
  .NET's `IsBasicLatin` block, which the regex crate doesn't know; it failed to
  compile, so every code name passed where the SDK rejects `1Sheet` or
  `Sheet 1`. Every constraint pattern must now compile, and a pattern's final
  `$` also matches before one trailing line feed, as in .NET.
- Large sheets validate 25–30% faster, natively and in WebAssembly: each
  constraint finds its attributes once, not by name per cell; plain parts skip
  per-text checks; short names compare without memcmp; entries inflate without
  a CRC pass.
- The WebAssembly file is 3.3 MB, 680 KB gzipped (from 3.9 MB, 850 KB): regex
  keeps only the Unicode classes the patterns use, and the name section is
  stripped.

npm 0.2.0 and crate 0.3.0 were tagged but never published; this release
replaces them.

## npm 0.2.0, crate 0.3.0

Excel and PowerPoint. A workbook (.xlsx, .xlsm, .xltx, .xltm, .xlam) or a
presentation (.pptx, .pptm, .potx, .potm, .ppsx, .ppsm, .ppam) is checked as
the SDK checks a `SpreadsheetDocument` or `PresentationDocument`, with the same
error ids, parts and XPaths:

- Semantic constraints run for the applications they name: Excel's for a
  workbook, PowerPoint's for a deck, Word's for a document.
- Each package class loads parts from its own factory, as the SDK's do; a
  data part referenced from a part that can't hold one is
  `Pkg_DataPartReferenceIsNotAllowed`.
- A cell's value must suit its type (`Sem_CellValue`), and a style index
  counts the elements the SDK counts.
- Zip entries and part names are read as .NET reads them: no CRC check, a
  name listed twice can't be opened, a part needs a URI name and a content
  type, relationship parts their namespace.

The CLI picks the package class from the extension. `check()` and npm's
`validate()` pick it from the main part's content type; `check_as()` and
`{ format: "word" | "excel" | "powerpoint" }` name it.

Agreement: every verdict, error id and XPath matches the SDK on its own
test files of all three kinds, 111 problem workbooks and decks, Apache POI's
and LibreOffice's 1,333 test files (`CORPORA=1 fuzz/agree.sh`, now in CI) and
3,000 mutants of them. The WebAssembly build grew 7 KB.

The crate's version changes because `validate_part` takes the package class.
