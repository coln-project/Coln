# Coln compiler Wasm library

This package can be compiled with the [GHC WebAssembly backend](https://downloads.haskell.org/ghc/latest/docs/users_guide/wasm.html) to produce a [WASI reactor module](https://github.com/WebAssembly/WASI/blob/wasi-0.1/application-abi.md#current-unstable-abi). Use `just npm-package` to turn it in to an NPM package. See [this example](../../examples/compiler) for how to consume that package. All necessary tools, including a Wasm-targeting GHC and Cabal, are provided by this repository's top-level Nix shell.
