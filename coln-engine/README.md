# Geomerge

Geomerge is envisioned to be the storage layer of the Geolog database engine.
Roughly it ought to support:

1. Efficient storage and query of data
2. Native version control
3. Concurrency at scale
4. Conflict resolution where appropriate

This repo as is now is only a store engine that allows you to give it a compiled
geolog theory, load it, and then add data to its tables.

Example of loading a schema and inserting several related rows. Type `/help`
for the full command list (for example `load-schema`, `add`, `dump-table`,
`dump-store`, and the batch form below).

A **batch** is a single multi-line statement: `begin transact;` … `commit;`. Each
line inside is an `add` that may bind the new row id to a name (`name = add
<table> values (...);`). These ids are assigned by the storage layer and can then
be referred to later on. For example, `e1 = add G.E values (g0 v1 v2);` is
referring to `v1` and `v2` as vertex ids previously inserted.
The whole batch is submitted when you end it with `commit;`.

The snippet below loads the `paths` theory, creates two graphs (`g0`, `g1`),
records `g1` as the designated graph for the `G0` and `G1` indices, adds two
vertices on `g0`, and adds one edge between them on `g0`.

After `commit`, inspect what landed with `dump-table <table>;` — for example
`dump-table Graphs;`, `dump-table G.V;`, or `dump-table G.E;` — or print the
entire store with `dump-store;`.

```
geomerge> load-schema tests/data/paths.json;
begin transact;
  g0 = add Graphs values ();
  g1 = add Graphs values ();

  i1 = add G0 values (g1);
  i2 = add G1 values (g1);

  v1 = add G.V values (g0);
  v2 = add G.V values (g0);

  e1 = add G.E values (g0 v1 v2);

commit;

persist paths.bin;
load-store paths.bin;
```

To get a violation of the law, say (Hom.V.total), change the line `i1 = add G0 values (g1);`
to `i1 = add G0 values (g0)` so that we do not have a morphism between G0 and G1.

## Test Coverage

This repo uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
for Rust test coverage.

Install it once:

```sh
cargo install cargo-llvm-cov
```

Generate an HTML coverage report:

```sh
./scripts/coverage.sh
```

The report is written to `coverage/html`. For CI-style output, run:

```sh
cargo llvm-cov --workspace --lcov --output-path lcov.info
```
