#!/bin/zsh

set -euo pipefail

# Sampling rate in Hz. Override with RATE=100.
RATE="${RATE:-1000}"
# perf output, folded stacks, and AI-facing summary paths.
PERF_DATA="${PERF_DATA:-perf.data}"
FOLDED="${FOLDED:-profile.folded}"
SUMMARY="${SUMMARY:-profile.summary.txt}"
# Call-graph mode: dwarf (default) or fp.
# For fp, the binary is rebuilt with force-frame-pointers.
CALL_GRAPH="${CALL_GRAPH:-dwarf}"
# Userspace-only event keeps kernel noise out of AI-facing stacks.
EVENT="${EVENT:-cycles:u}"

SCRIPT_DIR="${0:A:h}"

if [[ "$CALL_GRAPH" == "fp" ]]; then
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C force-frame-pointers=yes"
fi

# Line tables keep release-ish speed while giving perf usable symbols.
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build --release --features native

rm -f "$PERF_DATA"

coproc perf record \
  -F "$RATE" \
  -e "$EVENT" \
  --call-graph "$CALL_GRAPH" \
  -o "$PERF_DATA" \
  -- ../../target/release/coln-store --enable-sql-mode

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

# Drain remaining output (REPL exit + perf summary), then reap.
while read -p line; do print -r -- "$line"; done
wait

echo "collapsing $PERF_DATA -> $FOLDED" >&2
perf_script=(perf script -i "$PERF_DATA" --demangle)
if command -v rustfilt >/dev/null 2>&1; then
  "${perf_script[@]}" | rustfilt | python3 "$SCRIPT_DIR/fold_perf_script.py" -o "$FOLDED"
else
  "${perf_script[@]}" | python3 "$SCRIPT_DIR/fold_perf_script.py" -o "$FOLDED"
fi

echo "summarizing $FOLDED -> $SUMMARY" >&2
python3 "$SCRIPT_DIR/summarize_folded.py" "$FOLDED" -o "$SUMMARY"

echo "wrote $FOLDED ($(wc -l < "$FOLDED") stacks)" >&2
echo "wrote $SUMMARY (give this file to an AI, not the raw folded stacks)" >&2
