#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Coln contributors
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if command -v coln >/dev/null 2>&1; then
  coln_cli=(coln)
elif command -v coln-cli >/dev/null 2>&1; then
  coln_cli=(coln-cli)
else
  coln_cli=(cabal run coln-cli --)
fi

"${coln_cli[@]}" generate-ts "$script_dir/graph.coln" --output-dir "$script_dir/generated"
"${coln_cli[@]}" generate-ir "$script_dir/graph.coln" --output-dir "$script_dir/generated"
