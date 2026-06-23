#!/usr/bin/env bash
set -euo pipefail

if command -v coln >/dev/null 2>&1; then
  coln_cli=(coln)
elif command -v coln-cli >/dev/null 2>&1; then
  coln_cli=(coln-cli)
else
  coln_cli=(cabal run coln-cli --)
fi

"${coln_cli[@]}" generate-ts ./graph.coln --output-dir ./src/generated
"${coln_cli[@]}" generate-ir ./graph.coln --output-dir ./src/generated

