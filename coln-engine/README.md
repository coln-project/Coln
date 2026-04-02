# Geomerge

Geomerge is envisioned to be the storage layer of the Geolog database engine.
Roughly it ought to support:

1. Efficient storage and query of data
2. Native version control
3. Concurrency at scale
4. Conflict resolution where appropriate

This repo as is now is only a store engine that allows you to give it a compiled
geolog theory, load it, and then add data to its tables.

Example of loading a schema and then add some data to it in a transaction. Also
type `/help` to see what is available.

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
```
