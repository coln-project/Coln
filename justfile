# Task runner: https://github.com/casey/just
# From repo root:
#   just              # list available recipes
#   just check           # run all package checks

default:
    @just --list

check-haskell: (check "coln-compiler") (check "coln-cli") (check "coln-repl") (check "coln-ls") (check "fnotation") (check "diagnostician")

check-rust: (check "coln-store")

check-typescript: (check "coln-js-runtime")

check-licences:
    git ls-files -z '*.[hrt]s' | xargs -0 reuse lint-file

check-all: check-haskell check-rust check-typescript

check package:
    just -f packages/{{package}}/justfile check

fix package:
    just -f packages/{{package}}/justfile fix

fix-licenses:
    git ls-files -z '*.[hrt]s' | xargs -0 reuse annotate -c "Coln contributors" -l Apache-2.0

