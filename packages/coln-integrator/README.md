# coln-integrator

> The data integrator of the Coln language.

`coln-integrator` maintains _snapshots_ of relations. Relations can have two
data sources:

1. `coln-store` for base tables.
2. `coln-query` for incrementally maintained views.

The latter emits a delta stream of updates which needs to be _integrated_, that
is, turned into a materialized snapshot reflecting one consistent point in time,
hence the name also. For both data sources `coln-integrator` provides indexes
and a sorted table API to consume by mainly `coln-batch` to run ad-hoc batch
queries against the provided data.
