test:
    cabal test

check-format:
    git ls-files -z '*.hs' | xargs -P 12 -0 fourmolu --mode check
    cabal-gild --mode=check --input=$(ls *.cabal)

check: check-format test

format:
    git ls-files -z '*.hs' | xargs -P 12 -0 fourmolu --mode inplace
    cabal-gild --io=$(ls *.cabal)

fix: format
