#!/usr/bin/env bash
# Build the server, bundle it into client/server/<platform>/, then package the VS Code extension as a .vsix.
# Run from repo root. Linux only for now (server goes to client/server/linux-x64/).

set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Building geolog-lsp (release)..."
nix shell github:NixOS/nixpkgs/bcd464ccd2a1a7cd09aa2f8d4ffba83b761b1d0e#{zlib,zlib.dev} -c cabal build geolog-lsp

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
(cd "$ROOT/client" && nix shell github:NixOS/nixpkgs/bcd464ccd2a1a7cd09aa2f8d4ffba83b761b1d0e#nodejs -c npm install)

echo "==> Building client extension..."
(cd "$ROOT/client" && nix shell github:NixOS/nixpkgs/bcd464ccd2a1a7cd09aa2f8d4ffba83b761b1d0e#nodejs -c npm run compile)

# echo "==> Pruning dev dependencies (smaller .vsix)..."
# (cd "$ROOT/client" && npm prune --production)

echo "==> Packaging .vsix..."
(cd "$ROOT/client" && nix shell github:NixOS/nixpkgs/bcd464ccd2a1a7cd09aa2f8d4ffba83b761b1d0e#nodejs -c npx --yes @vscode/vsce package --allow-missing-repository)

echo "==> Done. .vsix is in client/"
ls -la "$ROOT/client"/*.vsix 2>/dev/null || true
