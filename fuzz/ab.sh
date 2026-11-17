#!/bin/sh
# ab.sh BASE NEW FILE.docx [ROUNDS]: paired A/B of two bench binaries
# (examples/bench), alternating fresh processes so machine noise lands on
# both. Prints each side's median and the median paired ratio NEW/BASE.
set -eu
base=$1; new=$2; file=$3; rounds=${4:-8}
for i in $(seq "$rounds"); do
  a=$("$base" "$file" 9 2>&1 | sed -n 's/.*median \([0-9.]*\)ms.*/\1/p')
  b=$("$new" "$file" 9 2>&1 | sed -n 's/.*median \([0-9.]*\)ms.*/\1/p')
  echo "$a $b"
done | python3 -c '
import sys, statistics as s
rows = [tuple(map(float, l.split())) for l in sys.stdin if l.strip()]
r = sorted(b / a for a, b in rows)
print(f"base {s.median(a for a, _ in rows):.1f} ms  new {s.median(b for _, b in rows):.1f} ms  ratio {s.median(r):.3f} (range {r[0]:.3f}-{r[-1]:.3f}, {len(r)} rounds)")'
