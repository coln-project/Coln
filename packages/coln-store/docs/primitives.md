# Storage Primitives for Coln Store

This doc defines the storage primitives exposed by Coln Store. Coln Store is the
storage engine for Coln. Coln itself can be viewed as a complex database with
rich language and features.

## Motivation

The main motivation follows from the design of Owen's Typescript
[FFI](https://manual.geolog.sgai.uk/000S/index.xml) for Coln, which intends to
the main public API for other people to use Coln, and therefore I intend to
co-design the storage primitives around this FFI interface. Although right now
the queries are simple, such as checking if x exists by its rowid, such that they
can be done in Coln Store. In the future complex law checking might require
coordination with execution components outside the store. Motivated by this, we
shall layout the primitives/public APIs that Coln Store should expose. The main
purposes of this doc are therefore:

1. Document what primitives would be exposed by the storage engine. whilst the
storage's API might be quite low-level and not directly used by end-user, it
gives the Coln team members a taste of what it (would) look like and point out
where it is lacking;
2. Invite discussions around the design of such APIs. while we do want to have a
clear boundary, this is by no means final, and should evolve as other parts of
Coln evolves;
3. Add some thoughts on how we could integrate the Coln Store primitives with the
TS bindings.

This document will mainly focus on the query-related interface, i.e. CRUD. The
version control aspect of Coln Store is not covered here (but will be covered in
the future!).

## Primitives

I will write down the signatures in (roughly) Rust syntax, which should be accessible
enough to most of the audience. But I am open to suggestions on more accessible
format as well.

We can base our primitives roughly on the boring but well-established CRUD model.

- Create: creating a database is currently only possible with a given Coln theory.
See `Store::try_from_theory`. A Coln theory is like a SQL schema, but with
richer type system support. Such a theory therefore requires the compilation
from a compiler before it can be used by Coln Store. Supporting creating tables
directly might introduce complex schema violations that is not easily checkable
by Coln Store.

- Read: There will be two main read endpoints (unimplemented!), as refelected
by the Typescript binding

```rust
Store::scan_table(table_path) -> Result<impl Iterator<Item = RowView>, ReadError>
Store::row_by_id(path, row_id: RowId) -> Result<Option<RowView>, ReadError>
```

The first one `scan` gives an iterator to the underlying table values and the
second one is a member query as indexed by the row_id. Note this `row_by_id` is
only intended to support the most straightforward lookup right now, i.e. a table
storing edb. Although I have not thought about this in detail, but it is not
intended for derived tables that might involve the execution engine, unless the
results happens to be cached in Coln Store.

- Update:

Updates to a Coln database will be done through transactions. The transaction
API looks like this

```rust
Store::transaction() -> Transaction
Transaction::add(table, values) -> Result<TempRowId, StoreError>
Transaction::commit() -> Result<CommitHash, StoreError>
```

A transaction is performed on a store, which is a collection of tables as defined
by a Coln theory. Once obtained a Transaction object, a user can then call `add`
on it to add values into a particular table.  A user can perform any number of
adds to a store as wish, and then call `Transaction::commit()` to commit a transaction,
where checks against the database schema is performed. For example, the values
parameter will be checked against the table's schema and would be rejected if
there is mismatch. For a more concrete example, see also `test_path::add_basic_data_to_path()`
in `test_path.rs`

This is currently the only API to modify the database, and a "dirty" write API
is not planned to be supported, for several reasons:

1. Coln Store includes a versioned storage engine, which means users can identify a version
by its hash, merging different versions across agents or network, etc. A
transaction maps cleanly to an individual commit. The alternative
would be to introduce implicit transactions if we were to support "dirty" writes
outside of a transaction context;
2. The transaction commit point acts as a natural point for store integrity checking.
Although currently `Transaction::add()` does some preliminary check of value arity
for ergonomic reasons, the majority of the check still happens at commit time.
This means that law checking, as done by the Coln execution engine is triggered
at the end of each commit, and temporary law violation is necessarily
allowed when the commit is not finished yet. The law checking will
probably be implemented as a hook/call into the execution engine at
commit time. This feature is not implemented yet, and will be when we
come to integrate the storage engine with the execution components.

And to quote from the Coln manual
> Thus, validity is only checked at certain points; we discuss validity more in a later section

I think that the transaction commit point should be the validity checking point[^1].

- Delete:

Deletion is not supported yet, but will be in the future. From the user's perspective,
this will likely just be another `remove(table, id)` method inside a transaction
context. Internally the deletion might be implemented as a tombstone operation,
but the user does not need to worry about such storage semantics.

## Integration with Typescript FFI

Regarding the integration with the Typescript FFI, two of the read APIs should
be fairly straightforward to integrate as they pretty much map directly to the
Typescript API:

```ts
interface ReadonlySet {
  has(x: Value): boolean;
  values(): Iterator<Value>;
}
```

The write interface is not dissimilar, at least when the table consists of just
the rowid column, which means the transactional add takes a vec of empty values.

```ts
interface ReadWriteSet extends ReadonlySet {
  add(): Value;
}
```

For more complex tables with more than one column, the TS code looks like the
following. I guess with more columns, we would have something like `m.next(a)(b)(c).set(d)`.
This can be turned into Coln Store's add API as something like `txn.add(m, [a, b, c, d])`,
although there is quite a bit of code generation work/compiler work to be done,
which is probably where the majority of the work for the FFI binding to work,
but the mapping should not be too complicated.

```text
theory StateMachine := sig
  state : Set
  next : state -> state
end
```

```ts
const m = StateMachine.create()
const [x,y] = [m.state.add(), m.state.add()]
m.next(x).set(y)
m.next(y).set(y)
```

And the relation API should be similar to above, a `jealous_of(a)(b).makeTrue()`
would just be `[a, b]`, and the presence of this indicates that the relation is true.

```ts
interface ReadonlyProp {
  isTrue(): boolean;
}

interface ReadWriteProp extends ReadonlyProp {
  makeTrue(): void;
}
```

Finally we should have something similar to transactions in the
Typescript binding, for the two reasons listed above (we have `OwnedTransaction`
for this purpose).  Automerge has Autocommit which manages the transaction for
the user, and we could have something similar for convenience purposes, but should
still keep the raw transactional API so that the user will think more carefully
about how they want to interact with a Coln database instance.

[^1]: There will be more complexity when we come to annotate whether a law is
chased, monitored, or something else. But that should not affect when we do the
law checking.
