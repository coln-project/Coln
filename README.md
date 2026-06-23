# Coln

Coln is a data-oriented proof assistant. We have a [work-in-progress manual](https://coln-project.github.io) and a [black triangle demo](https://coln-web-demo.netlify.app) (see [The Black Triangle](https://robinrendle.com/notes/the-black-triangle/) for an explanation of what a black triangle demo is). The software is also heavily pre-alpha.

## Quick Start

If you have nix, you can either download this repository and run `nix run .` from the repository root, or not even download the repository and run `nix run github:coln-project/Coln`.

If you don't have nix, you can install `cabal` and possibly some native dependencies (at least `zlib`), and then run `cabal run coln-cli`.

This will allow you to:

- Type check coln files: `coln check theories.coln`
- Generate TypeScript definitions from coln files: `coln generate-ts theories.coln -o OUT_DIR`
- Generate JSON IR (used by the storage engine): `coln generate-ir theories.coln -o OUT_DIR`
- Run a repl: `coln repl`
- Run a language server: `coln language-server`
