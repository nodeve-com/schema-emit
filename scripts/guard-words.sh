#!/bin/sh
# The crates stay general: no word of a product built on them.
set -e
cd "$(dirname "$0")/.."
words=$(grep -rnEiw 'loom|bundles?' src tests/*.rs tests/suite README.md MISSING.md Cargo.toml \
  schema-emit-runtime/src schema-emit-runtime/README.md schema-emit-runtime/Cargo.toml || true)
[ -z "$words" ] || { echo "$words"; exit 1; }
