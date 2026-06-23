let fourmolu_errors = (
  ls -a **/*.hs
  | each { |f|
      try {
        fourmolu --mode check --indentation 2 $f.name o+e> /dev/null
        null
      } catch { $f.name }
    }
  | compact
)

if ($fourmolu_errors | is-not-empty) {
  $fourmolu_errors | each { |f| print $"fourmolu error in ($f)" }
  exit 1
}
