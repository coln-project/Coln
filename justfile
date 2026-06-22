# Task runner: https://github.com/casey/just
# From repo root:
#   just              # list available recipes
#   just check           # run all package checks
#   just store-check  # fmt-check + clippy + test for coln-store
#   just store-test   # run coln-store tests
#   just store-fix    # apply cargo fixes + format for coln-store
#   just js-runtime-check  # rust + npm tests for coln-js-runtime

default:
    @just --list

check: store-check js-runtime-check

store-check:
    just -f packages/coln-store/justfile check

store-test:
    just -f packages/coln-store/justfile test

store-fix:
    just -f packages/coln-store/justfile fix

js-runtime-check:
    just -f packages/coln-js-runtime/justfile check
