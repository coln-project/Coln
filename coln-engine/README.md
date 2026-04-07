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
`dump-store`, and the transactional form below).

A **transaction** in the REPL is a single multi-line statement: `begin transact;`
… `commit;`. Each line inside may bind the new row id to a name (`name = add
<table> values (...);`). Those names act as variables for the rest of the
transaction, so later rows can refer to graphs and vertices inserted earlier.
The block is submitted when you end it with `commit;`.

The snippet below loads the `paths` theory, creates two graphs (`g0`, `g1`),
records `g1` as the designated graph for the `G0` and `G1` indices, adds two
vertices on `g0`, and adds one edge between them on `g0`.

After `commit`, inspect what landed with `dump-table <table>;` — for example
`dump-table Graphs;`, `dump-table G.V;`, or `dump-table G.E;` — or print the
entire store with `dump-store;`.

```
geomerge> load-schema tests/data/paths.json;
begin batch;
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
