# Changelog

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
