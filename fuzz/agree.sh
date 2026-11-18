#!/bin/sh
# agree.sh [VERSION]: checks ooxml-validator against Microsoft's Open XML SDK
# at VERSION (default: SDK_VERSION). Clones the SDK at that tag, regenerates
# the schema tables from it, builds the SDK oracle against that NuGet release,
# and diffs every verdict on the SDK's own test documents (Word, Excel and
# PowerPoint), mutants of them, fuzz/cases.py's and fuzz/cases_office.py's
# problem documents and a generated report. Exits 1 on any disagreement or on table drift.
set -eu
cd "$(dirname "$0")/.."
version=${1:-$(cat SDK_VERSION)}
work=${WORK:-$(mktemp -d)}
sdk=$work/sdk-$version
[ -d "$sdk" ] || git clone -q --depth 1 --branch "v$version" https://github.com/dotnet/Open-XML-SDK "$sdk"

cargo run -q --release -p gen -- "$sdk" src/schema_gen.rs
cargo build -q --release
(cd fuzz/oracle && dotnet build -c Release -o "$work/oracle" -p:OpenXmlVersion="$version" -v quiet -nologo)

docs=$work/docs
mkdir -p "$docs"
find "$sdk/test" -name '*.docx' | LC_ALL=C sort > "$docs/sdk.txt"
python3 fuzz/mutate.py "$docs/sdk.txt" "$docs/mutants" "${MUTANTS:-600}" "${SEED:-1}" > /dev/null
python3 bench/generate.py "$docs/report.docx" 40 > /dev/null
python3 fuzz/cases.py "$docs/cases" > /dev/null
# Workbooks and decks: the SDK's own, with CORPORA=1 Apache POI's and
# LibreOffice's test files (fetched, not vendored), mutants of them, and
# fuzz/cases_office.py's problem files.
find "$sdk/test" \( -iname '*.xlsx' -o -iname '*.xlsm' -o -iname '*.xltx' -o -iname '*.pptx' -o -iname '*.pptm' -o -iname '*.potx' \) | LC_ALL=C sort > "$docs/office.txt"
if [ "${CORPORA:-0}" = 1 ]; then
  fuzz/corpora.sh "$work/corpora" >> "$docs/office.txt"
fi
python3 fuzz/mutate.py "$docs/office.txt" "$docs/office-mutants" "${MUTANTS:-600}" "${SEED:-1}" > /dev/null
python3 fuzz/cases_office.py "$docs/office-cases" > /dev/null
cat "$docs/sdk.txt" "$docs/mutants/list.txt" "$docs/cases/list.txt" "$docs/office.txt" "$docs/office-mutants/list.txt" "$docs/office-cases/list.txt" > "$docs/all.txt"
echo "$docs/report.docx" >> "$docs/all.txt"
echo "SDK $version: $(wc -l < "$docs/all.txt" | tr -d ' ') documents"

status=0
ORACLE=$work/oracle/Oracle.dll python3 fuzz/diff.py "$docs/all.txt" "$work/diff" --jobs "${JOBS:-4}" --xpath || status=1
if ! git diff --quiet src/schema_gen.rs; then
  echo "schema tables differ from SDK $version: commit the regenerated src/schema_gen.rs"
  status=1
fi
exit $status
