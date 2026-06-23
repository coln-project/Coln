let cabal_errors = (
  ls -a **/*.cabal
  | each { |f|
      try {
        cabal-gild -m 'check' -i $f.name o+e> /dev/null
        null
      } catch { $f.name }
    }
  | compact
)

if ($cabal_errors | is-not-empty) {
  $cabal_errors | each { |f| print $"cabal-gild error in ($f)" }
  exit 1
}
