# Geolog

See [the prospectus](https://geolog.sgai.uk/prospectus.pdf) for an overview of the goals of the geolog project.

This is the *monorepo* for geolog development, which means that it contains a variety of experiments, resources, documentation, notes, and, eventually, bits and pieces of a geolog implementation.

## Getting started

The shakefile (`Shakefile.hs`) is (should be) "command central" for everything in this repository. Currently it just controls geolog-lang and the website. In order to use ths shake file, you need:

- GHC 9.12
- Cabal
- [tectonic](https://tectonic-typesetting.github.io/en-US/) for building TeX

Then you can run:

- `./shake docs` builds the docs (which are deployed in ci to [geolog.sgai.uk](https://geolog.sgai.uk))
- `./shake format` formats all of the Haskell files using fourmolu (in ci, `./shake check` checks the formatting)
- `./shake check` runs tests and checks formatting

## Resources

Check out the [moodboard](https://git.sgai.uk/creators/geolog/-/wikis/Moodboard) for a list of references that may be useful in the development of geolog.

## Projects

When you add a new thing to the monorepo, try to update this list!

- `geolog-lang` is an implementation of an elaborator for the geolog type theory.
- `collaborative-geolog` is a vibecoded sketch for the backend version control system
- `felix-db` is a collection of experiments around datalog
- `toy-datalog` is a toy datalog implementation
- `toy-datalog-web` is a web interface for `toy-datalog`
- `geolog-lsp` implements the language server protocol for geolog
- `docs` contains djot files and tex files which are built in CI and deployed to [geolog.sgai.uk](https://geolog.sgai.uk).
