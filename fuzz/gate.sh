#!/bin/sh
# gate.sh LIST: rebuild, run the tests, then diff obstat against the SDK on
# LIST; fails unless every document agrees.
set -eu
cd "$(dirname "$0")/.."
cargo build -q --release
cargo test -q --release 2>&1 | grep -q "FAILED" && { echo "tests failed"; exit 1; }
out=$(mktemp -d)
python3 fuzz/diff.py "$1" "$out" --jobs "${JOBS:-8}" | tee "$out/summary"
python3 -c "import json,sys; r=json.load(open('$out/diffs.json')); sys.exit(1 if r else 0)" || { echo "disagreements: $out/diffs.json"; exit 1; }
