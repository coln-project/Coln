#!/usr/bin/env bash
set -euo pipefail

cabal run coln-cli -- generate-ts ./graph.glog --output-dir ./src/generated
cabal run coln-cli -- generate-ir ./graph.glog --output-dir ./src/generated

