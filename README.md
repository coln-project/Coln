# Geolog

See [the prospectus](https://geolog.sgai.uk/prospectus.pdf) for an overview of the goals of the geolog project.

## Getting started

Dependencies:

- A recent ghc, cabal
- [tectonic](https://tectonic-typesetting.github.io/en-US/) for building TeX

The shakefile (`Shakefile.hs`) is "command central" for everything in this repository.

- `./shake docs` builds the docs (which are deployed in ci to [geolog.sgai.uk](https://geolog.sgai.uk))
- `./shake format` formats all of the Haskell files using ormolu (in ci, `./shake check` checks the formatting)

Check out the issues for what needs to be done.
