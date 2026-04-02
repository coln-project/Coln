#!/usr/bin/env bash
# Build the server, bundle it into client/server/<platform>/, then package the VS Code extension as a .vsix.
# Run from repo root. Linux only for now (server goes to client/server/linux-x64/).

set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Building geolog-lsp (release)..."
cabal build geolog-lsp

PLATFORM="linux-x64"
SERVER_DIR="$ROOT/client/server/$PLATFORM"
mkdir -p "$SERVER_DIR"

BINARY="$(cabal list-bin geolog-lsp)"
if [[ ! -f "$BINARY" ]]; then
  echo "ERROR: No geolog-lsp binary found."
  exit 1
fi
echo "==> Copying binary to $SERVER_DIR/"
cp "$BINARY" "$SERVER_DIR/geolog-lsp"

echo "==> Installing client dependencies..."
(cd "$ROOT/client" && npm install)

echo "==> Building client extension..."
(cd "$ROOT/client" && npm run compile)

# echo "==> Pruning dev dependencies (smaller .vsix)..."
# (cd "$ROOT/client" && npm prune --production)

echo "==> Packaging .vsix..."
(cd "$ROOT/client" && npx --yes @vscode/vsce package --allow-missing-repository)

VSIX=$(ls client/*.vsix 2>/dev/null)
if [[ -z "$VSIX" ]]; then
  echo "ERROR: No .vsix file produced."
  exit 1
fi
echo "==> Done: $ROOT/$VSIX"
