# Task runner: https://github.com/casey/just
# From repo root:
#   just              # list available recipes
#   just check           # run all package checks

default:
    @just --list

check package:
    just -f packages/{{package}}/justfile check

fix package:
    just -f packages/{{package}}/justfile fix

check-haskell: (check "coln-compiler") (check "coln-cli") (check "coln-repl") (check "coln-ls") (check "fnotation") (check "diagnostician")

check-rust: (check "coln-store")

check-typescript: (check "coln-js-runtime")

check-licenses:
    git ls-files -z '*.[hrt]s' | xargs -0 reuse lint-file

check-all: check-haskell check-rust check-typescript

fix-haskell: (fix "coln-compiler") (fix "coln-cli") (fix "coln-repl") (fix "coln-ls") (fix "fnotation") (fix "diagnostician")

fix-licenses:
    git ls-files -z '*.[hrt]s' | xargs -0 reuse annotate -c "Coln contributors" -l "(Apache-2.0 OR MIT)"

