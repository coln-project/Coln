# coln-batch

The batch query engine for Coln. This first slice evaluates conjunctive
queries (joins with equality, set semantics) over `u64` relations, with
two interchangeable executors that are differential-tested against a
brute-force oracle.

## Layout

- `relation.rs` is the in-memory relation type (`u64` columns,
  column-major).
- `generate.rs` and `rng.rs` build deterministic test workloads, a
  cyclic triangle join and the acyclic e-matching pattern `f(α, g(α))`.
- `io.rs` saves and loads relations as Arrow IPC files, the test-data
  substrate.
- `table.rs` defines the `SortedTable` trait, the engine's only window
  onto stored data. Ships an in-memory implementation and a conformance
  checker (`check_contract`) that future storage back ends can run
  against their own indexes. The trait's rustdoc is the contract.
- `query.rs` represents conjunctive queries as data, atoms over named
  relations, deliberately mirroring FLIR's rule shape, plus a catalog.
- `binary_join.rs` and `generic_join.rs` are the two executors, a
  classic hash-join chain and a worst-case-optimal generic join.
- `reference.rs` is the brute-force oracle that defines correct results
  in tests.

## Examples

`examples/smoke.rs` is the smallest possible run, a three-edge graph
and one join, checkable by hand:

```sh
cargo run -p coln-batch --example smoke
```

```text
path [1, 3]
path [2, 4]
```

`examples/demo.rs` is the full pipeline at scale. It generates two
workloads with 10,000 planted matches each, round-trips them through
Arrow IPC files, answers the fixture queries, and cross-checks the
executors against each other and against the planted count. Every step
prints what it does and what it verified, ending in `All checks
passed.`:

```sh
cargo run -p coln-batch --example demo --release
```

## Testing

Three layers. Unit tests per module, differential tests (both executors
must agree with the oracle, `tests/differential.rs`), and randomized
differential tests over generated query shapes
(`tests/random_queries.rs`).

```sh
cargo test -p coln-batch                                 # fast suite
cargo test -p coln-batch --release -- --include-ignored --nocapture  # + 10M-row runs with timings
```

`just check` runs the same gate as CI (format, clippy, tests).

## Scope

Correctness only for now. Performance work is deliberately deferred and
marked with `TODO(perf)`. Real storage plugs in behind `SortedTable`
without touching the join code. Next step is recursion, semi-naive
evaluation to a fixpoint.
