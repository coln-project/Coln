# Transaction APIs in coln-bouncer

`coln-bouncer` provides a transactional API for users to input data. There are
two main considerations when designing such an API:

1. For rust users, it needs to provide static guarantees on state transitions,
e.g. one cannot abort a txn once it has committed, or cannot read data after
it has been aborted.
2. For FFI, it needs to accommodate the FFI author. TS's type system lacks some
features as compared to rust which lowers the guarantees provided by the rust
api. It also complicates what kind of methods can be bound, for example, methods
that consume self seem to be disliked by them...

## The state machine

Broadly speaking, coln-bouncer implements the following state machine for write
transactions:

```text
                       Bouncer::begin_write()
                                 │
                                 ▼
                  ┌──────────────────────────────┐
                  │           Prepare            │
                  └──────────────────────────────┘
                                 │
                                 │ try_commit() -> Ok(_)
                                 ▼
                  ┌──────────────────────────────┐
                  │           Pending            │
                  └──────────────────────────────┘
                         │                │
                commit() │                │ abort()
                         ▼                ▼
                  ┌─────────────┐  ┌─────────────┐
                  │  Committed  │  │   Aborted   │
                  └─────────────┘  └─────────────┘
```

- `Prepare` queues operations with `add`; `read` sees committed data only.
  at this stage no data is applied, they are just buffered, and cannot be read
  through the read APIs.
  In theory these data could be read as well, but it's very tricky to design such
  an API because they have different shapes than the data stored in tables. For
  example, they are not canonicalised, they don't have `PublicRowId`s, etc. I'll
  leave this for now unless there is a real need to do that.
- `try_commit` applies the operations tentatively to the store and runs them
  through the query engine. A hard constraint violation, or any store
  error, returns `Err` and leaves both engines unchanged.
- `Pending` holds the store's write lock and the query engine's rollback.
  `read` sees the tentative base data, but not derived data yet.
- `commit` commits the query engine, installs its derived data in the store
  and commits the store. `abort` rolls both back.
- Dropping `Prepare` discards queued operations. Dropping `Pending` rolls
  both engines back, just like `abort`.

## Examples

As an example use of the rust API, see the annotated `test_tc_computation` test
in test_tc.rs.

A second use the write transaction is to decode and apply some data arrived over
the network. See the `repeated_sync_is_a_noop_for_base_and_derived_data` test
in test_tc.rs. In this case it is not a state machine but just one step call
of `apply_chunk_bytes`, through `begin_import()`.

For read-only workloads, one needs to invoke `begin_read()` on a bouncer, which
will hand back a `ReadTxn` that can be used to read data. See also the test
`repeated_sync_is_a_noop_for_base_and_derived_data` for some read before syncing.


## Node/Js Bindings

For Node/Js bindings, I suggest we take two approaches: 1. use the sexpr based 
syntax for sending queries to the backend; 2. Javascript functions that build
up a query, in a style similar to Js/Ts query builders like Kysely or Knex.

My suggestion is support both of these ways of writing queries, as many query
builders do support SQL. The second one is more higher level than the relational level, a
bit like the object relational mapping one would find. But the cost of supporting
the lower level one does not seem to high. It might be that in order to support
the higher level interface, we would have already built enough infrastructure
to support the sexpr based syntax.

For the first one, example:

```javascript

const client = new Bouncer(ir);

const res = client.execute(`
(query
  ((%v0 'V) (%v1 'V) (%e1 'E))
  (('V (id %v0))
   ('V (id %v1))
   ('E (id %e1) %v0 %v1)))
`);

const res2 = client.execute(`
  (mutation
  (let %v0 (insert 'V))
  (let %v1 (insert 'V))
`)

// I reckon we could also support transactions where people can group multiple
// operations together

const res3 = client.transaction(client => {
  client`(mutation ...)` // use Js tagged templates here
  client`(query ...)`
});

```

A coln-bouncer cli (to be developed) will also support this syntax for
querying/mutating on it.

And the second one, for example:

```javascript

client = new Bouncer(ir);

client.transaction(root => {
  const v1 = root.V.add();
  const v2 = root.V.add();
  const e1 = root.E(v1)(v2).add();
});

client.all_proj() // for queries

```

These are the user-facing APIs, which abstracts away the state machine transition 
logic that is exposed by the rust api. So there will be a layer of abstraction
between this syntax and the underlying rust api. I expect that layer will be
messy :), but in a sense that will be the responsibility of coln developers to
handle that mesiness so that the users do not need to worry about it.

