#!/usr/bin/env bash
set -euo pipefail

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is not installed." >&2
  echo "Install it with: cargo install cargo-llvm-cov" >&2
  exit 1
fi

cargo llvm-cov --workspace --html --output-dir coverage/html "$@"
