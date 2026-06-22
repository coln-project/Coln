#!/usr/bin/env bash
set -euo pipefail

cabal run coln-cli -- generate-ts ./graph.coln --output-dir ./src/generated
cabal run coln-cli -- generate-ir ./graph.coln --output-dir ./src/generated

