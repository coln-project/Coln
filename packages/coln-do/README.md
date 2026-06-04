# Coln Automation

This is the one-stop shop for *all* automation within the coln project. This means that we should not have random bash scripts!

And yes, that means even for the automation over Rust, that should *still* go in `coln-auto`. You can write Shake (Make as a Haskell DSL) rules even if you don't know any Haskell, following the [Shake manual](https://shakebuild.com/manual).

It is not a bug that contributors who are only writing in Rust also have to install GHC and learn a little Haskell; it is a feature!

## How to run

`cabal run coln-auto -- TARGET`

## Targets

- `test`: runs all tests, TODO
  - `test-formatting` TODO
  - `test-compiler` TODO
  - `test-runtime` TODO
  - `test-store` TODO
  - `test-playground` TODO
- `build`: builds all binaries, TODO
  - `build-cli`, TODO
  - `build-playground`, TODO
- `site`: builds website, TODO
  - `manual`: builds manual
  - `docs`: builds docs from source code
    - `docs-haskell`: builds haddock docs
    - `docs-rust`: builds rustdoc docs
- `format`: autoformats the source, TODO
  - `format-haskell`: format haskell files with `fourmolu` and `cabal-gild`
  - `format-rust`: format rust files with `cargo fmt`
