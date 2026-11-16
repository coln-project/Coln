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
  through the read APIs
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
in test_tc.rs. In this case it is not a state macine but just one step call
of `apply_chunk_bytes`, through `begin_import()`.

For read-only workloads, one needs to invoke `begin_read()` on a bouncer, which
will hand back a `ReadTxn` that can be used to read data. See also the test
`repeated_sync_is_a_noop_for_base_and_derived_data` for some read before syncing.

## TODO items

- `coln-query` needs to expose methods to take deltas and violations at the pending
state to allow users to see the derived data
- Give examples of TS API