test:
    cabal test

check-format:
    git ls-files -z '*.hs' | xargs -P 12 -0 fourmolu --mode check
    cabal-gild --mode=check $(ls *.cabal)

check: check-format test

format:
    git ls-files -z '*.hs' | xargs -P 12 -0 fourmolu --mode inplace
    cabal-gild --mode=format $(ls *.cabal)

fix: format
