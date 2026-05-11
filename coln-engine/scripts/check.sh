#!/usr/bin/env bash
# Local pre-push checks aligned with .gitlab-ci.yml (fmt, clippy, test).
# Run: nix develop -c ./scripts/check.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
