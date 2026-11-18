#!/bin/sh
# corpora.sh DIR: fetches the .xlsx and .pptx test files of Apache POI
# (Apache-2.0) and LibreOffice (MPL-2.0) into DIR, at pinned commits, and
# prints their paths. They are not vendored: agree.sh reads them in place.
set -eu
dir=$1
mkdir -p "$dir"
fetch() { # name url commit paths...
  name=$1 url=$2 commit=$3; shift 3
  if [ ! -d "$dir/$name/.git" ]; then
    git init -q "$dir/$name"
    git -C "$dir/$name" remote add origin "$url"
    git -C "$dir/$name" config core.sparseCheckout true
  fi
  printf '%s\n' "$@" > "$dir/$name/.git/info/sparse-checkout"
  git -C "$dir/$name" fetch -q --depth 1 --filter=blob:none origin "$commit"
  git -C "$dir/$name" checkout -q FETCH_HEAD
}
fetch poi https://github.com/apache/poi f66927954a29cf1f5b86614fdf1816e6321c8aea test-data/spreadsheet/ test-data/slideshow/
fetch libreoffice https://github.com/LibreOffice/core 5b55ba4d4746b6ca0f19ab17f4f87865b6afc768 sc/qa/unit/data/xlsx/ sc/qa/unit/data/xlsm/ sd/qa/unit/data/pptx/ sd/qa/unit/data/xml/ oox/qa/unit/data/
find "$dir/poi" "$dir/libreoffice" -type f \( -iname '*.xlsx' -o -iname '*.xlsm' -o -iname '*.xltx' -o -iname '*.xltm' -o -iname '*.pptx' -o -iname '*.pptm' -o -iname '*.potx' -o -iname '*.ppsx' \) | LC_ALL=C sort
