#!/bin/zsh

set -euo pipefail

# Sampling rate in Hz. Override with RATE=100.
RATE="${RATE:-1000}"
# Profile output path when saving without opening the UI.
OUTPUT="${OUTPUT:-profile.json.gz}"

# Line tables keep release-ish speed while giving samply usable stacks.
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build --release --features native

samply_args=(record --rate "$RATE" --profile-name coln-store-copy -o "$OUTPUT")
if [[ "${SAVE_ONLY:-0}" == 1 ]]; then
  samply_args+=(--save-only)
fi

coproc samply "${samply_args[@]}" -- ../../target/release/coln-store --enable-sql-mode

# Send the load commands.
print -p "create table expr_app (id UUID, left UUID, right UUID);"
print -p "copy expr_app from 'data/expr_app.csv' with (format csv, header true);"

# Echo output until the copy confirmation appears, then exit so the profile
# covers the load rather than idle time.
while read -p line; do
  print -r -- "$line"
  [[ $line == *"copied"*"rows into expr_app"* ]] && break
done

print -p ".exit"

# Drain remaining output (REPL exit + samply messages), then reap.
while read -p line; do print -r -- "$line"; done
wait
