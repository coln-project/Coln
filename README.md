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

## Opinions and Resources

This is a collection of resources (blog posts, papers, example code) for learning about various subjects involved in geolog.

### Parsing

In general, our perspective on parsing is the following

1. Recursive descent is best
2. Parser combinators are a convenient way of doing recursive descent, but really one should aim to be LL(1) with respect to the tokenizer. This means that you write a function `lex1 :: Parser Token`, and then you make decisions about what to do next based on the returned token; decisions that you don't backtrack on.
3. Resolve precendence with Pratt parsing.
4. Parsing always returns a syntax tree (which might include error nodes). *Diagnostics* are a separate problem from errors; diagnostics are logged while parsing but don't change the semantics of parsing.
5. Parsing produces *notation*, which is much looser than syntax (https://parentheticallyspeaking.org/articles/bicameral-not-homoiconic/)

Resources:

- https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html. This is good to get the "vibe" of resilient parsing, but it's not necessary to have a full concrete syntax tree unless you are actually building an LSP, which is not our aim.
- https://github.com/ToposInstitute/fnotation
- https://github.com/olynch/sifaka2/tree/main/fnotation
- https://grugbrain.dev/#grug-on-parsing
- https://github.com/AndrasKovacs/flatparse
- https://github.com/flix/flix/blob/master/main/src/ca/uwaterloo/flix/language/phase/Parser2.scala

### Elaboration

Resources:

- https://davidchristiansen.dk/tutorials/nbe/
- https://github.com/AndrasKovacs/elaboration-zoo
- https://github.com/gwaithimirdain/narya
- https://github.com/AndrasKovacs/smalltt
- https://github.com/AndrasKovacs/2ltt-impl (this has the most up-to-date Andras Kovacs thought)
- https://github.com/RedPRL/cooltt
- https://github.com/ToposInstitute/emtt
- https://github.com/ToposInstitute/CatColab/tree/main/packages/catlog/src/tt

### Datalog

Resources:

- [Paris Koutris' lecture notes](https://pages.cs.wisc.edu/~paris/lecture-notes/)
