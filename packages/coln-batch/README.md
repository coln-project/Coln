# coln-batch

The batch query engine for Coln, built bottom-up. Current state:

- [x] **AP1** — deterministic generators for e-matching-style join
  workloads (`src/generate.rs`: cyclic triangle, acyclic `f(α, g(α))`),
  saved/loaded as Arrow IPC files (`src/io.rs`).
- [x] **AP2** — the `SortedTable` trait: the interface through which the
  engine reads relations (`src/table.rs`), an in-memory implementation
  built from Arrow data, and a conformance checker. Contract:
  [docs/sorted-table-api.md](docs/sorted-table-api.md).
- [x] **AP3** — query representation (`src/query.rs`: conjunctive queries
  as data, mirroring FLIR's rule shape; catalog; fixtures in
  `src/fixtures.rs`).
- [x] **AP4** — executor 1: binary hash-join chain
  (`src/binary_join.rs`).
- [x] **AP5** — executor 2: worst-case-optimal generic join
  (`src/generic_join.rs`); differential-tested against executor 1 and a
  brute-force oracle (`src/reference.rs`, `tests/differential.rs`).
- [x] **AP6** — end-to-end example (`examples/demo.rs`: generate → Arrow
  IPC round-trip → generic join → result sizes and timings).
- [ ] **AP7** — recursion: semi-naive evaluation to a fixpoint.

## Usage

```sh
cargo test -p coln-batch                              # fast tests
cargo test -p coln-batch --release -- --include-ignored # + 1M-row runs
cargo run -p coln-batch --example demo --release      # end-to-end demo
```
