#!/bin/zsh

set -euo pipefail

PLATEAU_SEC="${PLATEAU_SEC:-60}"

cargo build --release --features native

coproc ../../target/release/coln-store --enable-sql-mode

# Send the load commands.
print -p "create table expr_app (id UUID, left UUID, right UUID);"
print -p "copy expr_app from 'data/expr_app.csv' with (format csv, header true);"

# Echo output until the copy confirmation appears. This is the sync point:
# we only start the plateau once the store is fully loaded.
while read -p line; do
  print -r -- "$line"
  [[ $line == *"copied"*"rows into expr_app"* ]] && break
done

print -p ".save expr.colnstore
print -p ".exit"

# Drain remaining output so heaptrack's summary is visible, then reap.
while read -p line; do print -r -- "$line"; done
wait
