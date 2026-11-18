<h1 align="center">ooxml-validator</h1>
<p align="center">Microsoft's Open XML SDK validator for Word documents, rewritten in Rust: the same verdicts, 24× faster, no .NET.</p>
<p align="center">
  <a href="#use-it">Get started</a> ·
  <a href="#how-it-agrees">How it agrees</a> ·
  <a href="#how-fast">Benchmarks</a> ·
  <a href="DESIGN.md">How it works</a>
</p>

![The Open XML SDK and ooxml-validator each checking a 400-page report and then a broken file: the same verdicts, the SDK in 1.8 s, ooxml-validator in 73 ms](docs/demo.gif)

*A real terminal session: the SDK runs through the small C# program in `fuzz/oracle`, then
`ooxml-validator` runs on the same files.*

## Use it

```sh
npm install @2018kguo/ooxml-validator      # WebAssembly, any OS, Node 20+
cargo install ooxml-validator              # a native binary
```

```sh
$ ooxml-validator report.docx broken.docx
report.docx: ok
broken.docx: 3 errors
  /word/document.xml w:rPr Sch_UnexpectedElementContentExpectingComplex: The element 'w:rPr' has unexpected child element 'w:b'.
  /word/styles.xml w:shd Sch_MissRequiredAttribute: The required attribute 'val' is missing.
  /word/styles.xml w:tblPr Sch_UnexpectedElementContentExpectingComplex: The element 'w:tblPr' has unexpected child element 'w:tblBorders'.
```

It exits 1 when any file is invalid. `--json` prints one object per file with every error's id,
description, part and element, and `--max-errors N` caps the list (default 1000, as the SDK does).

From JavaScript:

```js
import { readFile } from "node:fs/promises";
import { validate } from "@2018kguo/ooxml-validator";

const report = await validate(await readFile("report.docx"));
// { ok: false, openError: null, errors: [{ id, description, part, node, errorType, xpath }, ...] }
```

`validate` runs on the thread that calls it. A server should keep that off its event loop:

```js
import { createValidatorPool } from "@2018kguo/ooxml-validator/worker";

using pool = createValidatorPool({ threads: 2, onReplace: (reason) => metrics.count(reason) });
const report = await pool.validate(bytes, { maxErrors: 50, timeoutMs: 15_000 });
```

A call past `timeoutMs` rejects with a `TimeoutError` and its worker is replaced; `onReplace` hears
every replacement (`"timeout"`, `"exit"`, or `"memory"` for a worker retired after a large document).

From Rust:

```rust
let report = ooxml_validator::check(&std::fs::read("report.docx")?);
for e in &report.errors {
    println!("{} {} {}: {}", e.part, e.node, e.id, e.description);
}
```

## What it checks

Everything `OpenXmlValidator(FileFormatVersions.Microsoft365)` checks in a .docx, with the SDK's
error ids:

- **The schema:** child order and counts, required and undeclared attributes, every simple type as
  .NET parses it (`Int32`, on/off, hex, unions, patterns, lengths), version-gated elements and
  enum values.
- **Markup compatibility:** `mc:Ignorable`, `mc:ProcessContent`, `mc:AlternateContent`.
- **Semantic constraints:** unique ids, value ranges, attributes that require or exclude each
  other, relationship ids and types, references into other parts.
- **The package:** required, allowed and repeated parts, and everything that keeps the SDK from
  opening a file at all: XML that isn't well formed, a missing part, a wrong content type, a bad
  relationship, an encoding .NET doesn't read.
- **Strict documents (ISO 29500):** read as Transitional, as the SDK reads them.

Descriptions follow the SDK's wording without its long expected-element lists. An error names its
element and gives the SDK's XPath to it (`/w:document[1]/w:body[1]/w:p[3]`), checked against the SDK
with the error ids. A parse error carries its line and column instead.

## How fast

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/benchmarks-dark.svg">
  <img src="docs/benchmarks-light.svg" alt="Bar charts: a 400-page report validates in 47 ms with ooxml-validator and 1.14 s with the Open XML SDK; 419 documents in 181 ms and 5.26 s; peak memory 13 MB and 143 MB.">
</picture>

Measured on an Apple M5 Pro, the median of 5 runs, each a fresh process so startup counts:

| | ooxml-validator | WebAssembly (Node) | Open XML SDK |
|---|---:|---:|---:|
| 400-page report | 47 ms, 13 MB | 216 ms, 87 MB | 1.14 s, 143 MB |
| 419 SDK test documents | 181 ms, 16 MB | 547 ms, 95 MB | 5.26 s, 159 MB |

The SDK side is published ReadyToRun, its fastest setup. To rerun it:

```sh
python3 bench/generate.py report.docx 400
python3 bench/compare.py --sdk "dotnet fuzz/oracle/bin/out/Oracle.dll" \
  --ours target/release/ooxml-validator --wasm "node npm/cli.js" \
  --large report.docx --corpus list-of-docx-paths.txt
```

The SDK builds a DOM of the whole document and, for each value that fails a union member, a
localized error message it then discards. ooxml-validator reads each part once as a stream of
borrowed bytes, with names and namespaces turned into integers when they are declared, and checks
parts on several threads. [DESIGN.md](DESIGN.md#speed) has the changes that mattered.

## How it agrees

The SDK is the reference, quirks included, and the tests check against the SDK itself:

- `gen/` turns the SDK's own schema data, part definitions and constraint registrations into
  static tables, and stops if the SDK adds something it doesn't know how to port.
- `fuzz/oracle` is a 60-line C# program around the SDK. `fuzz/diff.py` runs both on a list of files
  and fails on any document where the verdict or the set of error ids differs.
- `fuzz/agree.sh` checks the SDK's own test documents, 600 mutants of them, 97 documents with the
  problems real producers write (LibreOffice, JavaScript and PDF converters) and a generated
  report. CI runs it on every push.
- Every week CI fuzzes with a new seed and tries the newest SDK release on NuGet. A release that
  agrees becomes a pull request, one that doesn't becomes an issue.
- Before release it also agreed with the SDK on 9,337 documents in all: those, plus more than
  7,000 real-world files and their mutants.

## Limits

- Word documents only (.docx, .docm, .dotx). Not Excel or PowerPoint.
- The Microsoft365 file format only, the newest the SDK knows.
- Error descriptions and order differ from the SDK's; the set of error ids doesn't.
- Bounded where the SDK isn't: a .docx over 100 MB, or more than 128 MB of XML in
  all, is refused with an open error; each part keeps its first 1,000 errors (the
  SDK stops at 1,000 in all); the WebAssembly build stops at 1 GiB of memory.
- Pinned to SDK 3.5.1 until the weekly job proposes a newer one.

## License

MIT. The schema tables are generated from the [Open XML SDK](https://github.com/dotnet/Open-XML-SDK)'s
data, MIT-licensed by the .NET Foundation; [NOTICE](NOTICE) has its license.
