# Coln Automation

At one point, the idea was to have all automation for the coln project go through this. However, it is much faster in CI to have dependencies cached through nix, and thus in order to avoid duplication we are going to do automation through nix instead.

## How to run

The following command will place the `coln-do` binary in `bin/cdo`. Then if you have [direnv](https://direnv.net/) installed, this will put `cdo` in your path.

`cabal run coln-do -- install-self`

Then you can run various targets with `cdo TARGET`.

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
