# Coln compiler Wasm library

This package can be compiled with the [GHC WebAssembly backend](https://downloads.haskell.org/ghc/latest/docs/users_guide/wasm.html) to produce a [WASI reactor module](https://github.com/WebAssembly/WASI/blob/wasi-0.1/application-abi.md#current-unstable-abi). See [this example](./example.html) for how to call it from JavaScript. The example app can be launched with `just serve-example`. All necessary tools, including a Wasm-targeting GHC and Cabal, are provided by this repository's top-level Nix shell.
