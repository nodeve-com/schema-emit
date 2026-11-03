#!/bin/sh
# Instructions each read of each cost case executes, by callgrind, collecting only inside
# `measured_decode`: the case's decode, the derive reference's, and serde_json's Value.
# Prints the total and the costliest functions by their own instructions. Linux with valgrind;
# elsewhere, through scripts/linux.sh. The cost test builds the program, unstripped.
set -e
cd "$(dirname "$0")/.."
cargo test --locked --quiet --test cost >/dev/null
exe=${CARGO_TARGET_DIR:-target}/tmp/schema-emit-cost/target/release/schema-emit-cost
out=${CARGO_TARGET_DIR:-target}/tmp/schema-emit-cost
for case in $("$exe" --list); do
  for kind in decode derive value; do
    log="$out/$case-$kind.callgrind"
    valgrind --tool=callgrind --toggle-collect='*measured_decode*' --callgrind-out-file="$log" \
      "$exe" "$case" "$kind" >/dev/null 2>&1
    echo "## $case $kind"
    callgrind_annotate "$log" | sed -n '/PROGRAM TOTALS/p;/^Ir  *file:function/,$p' |
      sed 's| \[.*\]$||' | head -n 8
  done
done
