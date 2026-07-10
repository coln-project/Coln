# Sorted-table API — draft v0

**Status:** working draft, 2026-07. **From:** batch query engine (Jan).
**To:** storage/index layer (Leo Stewen, Vincent Liu). **Goal:** agree on
the smallest interface through which the batch engine reads relations, so
that engine and storage can be developed independently.

The Rust definition of record is `coln_batch::table::SortedTable`
(`packages/coln-batch/src/table.rs`). This document explains the contract
and the reasoning. For v0 the engine runs on generated Arrow test data and
an in-memory implementation (`ArrowSortedTable`); the intent is that a
Hexane-backed index can replace it without touching any join code.

## The one concept: a sorted view of a relation

One `SortedTable` is one **index**: all rows of a relation, totally
ordered by the lexicographic order of a declared column permutation
(`sort_order`). Example: with `sort_order = [1, 0]`, rows are sorted by
column 1, ties broken by column 0. This matches FLIR's
`Index { method: BTree, columns }` schema entries.

The engine never sees pages, chunks, or physical encodings. It asks five
things:

| method | meaning | expected cost |
|---|---|---|
| `arity()` | number of columns | O(1) |
| `len()` | number of rows | O(1) |
| `sort_order()` | the column permutation rows are sorted by | O(1) |
| `value(row, col)` | cell at sorted position `row`, schema column `col` | O(1) amortized |
| `lower_bound(depth, v, lo, hi)` | first position in `lo..hi` whose value in the `depth`-th **sort** column is `>= v` | O(log(hi−lo)), better if you can |

`upper_bound` and `equal_range` are derived from `lower_bound` and have
default implementations; back ends may override them.

**Precondition on searches:** callers only search within a range whose
rows already agree on sort columns `0..depth`. The engine descends the
sort order left to right, so this always holds — implementations may rely
on it (e.g. a B-tree can descend level by level).

**Values are `u64` for now.** Row ids are u64-like anyway; literals will
eventually be dictionary-encoded to u64. No nulls: FLIR atoms are total
rows.

## Why exactly these operations

- **Binary hash joins** need only sequential reads: `value` row by row.
- **The worst-case-optimal generic join** solves a multiway join one
  *variable* at a time. For each variable it intersects the candidate
  values of all relations containing that variable by leapfrogging:
  repeated `lower_bound`/`equal_range` calls, each within the row range
  fixed by the values chosen for earlier variables. Sorted order plus
  prefix seek is the entire requirement — no hash tables on the storage
  side.
- **Semi-naive recursion** (September milestone) adds no new read
  operation; the engine materializes its delta relations itself in v0.

`coln_batch::table::check_contract` is a brute-force conformance test
that walks ranges exactly the way the generic join does. Point it at any
implementation of the trait to validate it.

## What the storage side decides freely

- Physical layout (Hexane chunks, B-trees, sorted runs), caching,
  compression.
- How multiple sort orders of one relation are realized: fully
  materialized copies, permutation indexes, or something else. The engine
  only ever asks for *a* `SortedTable` with a given `sort_order`.

## Out of scope for v0 (deliberately)

- Updates and incremental view maintenance (that is the DBSP/IVM track).
- Version history, branches, commits; deletions/tombstones.
- Literal types beyond u64, value dictionaries.
- Concurrency and snapshots: v0 assumes one immutable snapshot per run.

## Open questions for v1

1. **Which sort orders exist?** The engine's planner will request one
   order per relation compatible with the chosen global variable order.
   Interim assumption: storage builds indexes on demand or ahead of time
   from FLIR `Index` declarations.
2. **Batched access.** `value()` is per-cell; a block API (contiguous
   `&[u64]` runs per column for a row range) would enable vectorization.
   Worth adding once the engine is correct.
3. **Fast seeks.** Galloping/exponential search inside Hexane chunks vs.
   plain binary search — relevant for skewed leapfrogging.
4. **Delta relations during recursion:** engine-materialized in v0;
   revisit when the IVM track lands.
