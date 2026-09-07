---
name: coln-repo
description: Read and update a synchronized Coln store from a coln: URL with the coln-repo CLI. Use whenever a request names a coln: document, a Coln store, realm, or theory, or asks to add, list, count, or connect records or table entries in one.
---

# Working With a Coln Store

A Coln document is addressed by a `coln:` URL and holds a store: a set of
tables whose paths come from a compiled theory, such as `Records.Documents`. The
`coln-repo` CLI reads and updates one such document. Every response is a single
JSON object on stdout, including errors. Run `coln-repo help` for usage and
`coln-repo guide` to print this document.

## Procedure

### 1. Resolve the sync endpoint

Pass `--endpoint <ws-url>` when the user names a relay. Otherwise the CLI uses
`SUBDUCTION_ENDPOINT`, then the public relay `wss://coln.sync.inkandswitch.com`.
A document held by a local relay is only reachable there; do not invent one. If
the response is `DOCUMENT_OPEN_FAILED`, report which endpoint you tried and ask.

### 2. Read the IR

```bash
coln-repo ir --document <coln-url> [--endpoint <ws-url>]
```

The `ir` field is the compiled JSON IR, unchanged. Each entity has a `path`
(render `[["Records"],["Documents"]]` as `Records.Documents`), ordered `columns` with a
`type` of either `{ tag: "builtin", type: "builtinInt" | "builtinString" }` or
`{ tag: "rowId", path }`, and an optional `primaryKey`. Rules describe
constraints the store enforces at commit, such as foreign keys.

Treat the IR as structural evidence, not domain documentation. A table name or
abbreviation does not establish what its rows represent.

### 3. Infer a mapping and confirm it

Never assume that names or abbreviations carry a particular meaning. Before
writing, state the mapping you inferred and ask the user to confirm it,
including argument order when column names do not explain it:

> The IR defines `Records.Folders` with a `name` string and
> `Records.Documents` with `folder` referencing `Records.Folders` followed by a
> `title` string. I take "add a document to Inbox" to mean adding or finding the
> Inbox folder, then adding a document as `[folderRowId, title]`. Is that right?

If several entities plausibly match, ask; do not pick the closest name. Copy
entity paths exactly from the IR.

### 4. Query before writing when the request refers to existing rows

```bash
coln-repo query --document <coln-url> <<'JS'
store.scanTable("Records.Folders")
JS
```

`query` reads one synchronous JavaScript expression from stdin. `store` exposes
`scanTable(path)`, `rowById(path, rowRef)`, `heads()`, and `jsonIR()`. Rows are
`{ rowId, values }`; a `rowId` is already a tagged `row_id` value and can be
passed straight back as a column value in `txn.add`. `rowById` takes the inner
reference instead: `store.rowById(path, row.rowId.value)`. Project or slice
large tables in the expression rather than returning everything.

### 5. Write in one transaction

```bash
coln-repo exec --document <coln-url> <<'JS'
const folder = txn.add("Records.Folders", [{ tag: "string", value: "Inbox" }])
const document = txn.add("Records.Documents", [
  folder,
  { tag: "string", value: "Notes" },
])
return { folder, document }
JS
```

`exec` reads a synchronous JavaScript function body from stdin. `txn` is the
current transaction API; today it exposes `txn.add(path, values)`, which returns
a complete tagged `row_id` value usable directly in later `add` calls within
the same script. Do not wrap that result in another `row_id` value. Return a
JSON-serializable value: it is included in the response with row references
resolved to their committed form.

Column values are tagged:

```js
{ tag: "string", value: "hello" }
{ tag: "int", value: 42 }
{ tag: "row_id", value: rowReference }
```

### 6. Read the response before doing anything else

Success: `ok: true`, `document.headsBefore` and `headsAfter`, `sync.status`, and
`result`.

Failure: `ok: false` and `error` with `code`, `phase`, `message`, and
`mutationState`:

- `not_applied`: nothing was committed. Fix the cause before trying again; a
  schema or rule rejection will not pass on retry.
- `applied_locally`: the transaction committed but synchronization or result
  reporting failed afterwards. Never repeat the transaction automatically; tell
  the user what committed and what failed.
- `unknown`: internal failure; report it.

### 7. Verify

Confirm a write with a `query`, for example `store.scanTable(path).length`, and
quote the returned row references to the user.

## Limits

- Add-only: `txn.add` is the whole transaction API today. Say so rather than
  simulating updates or deletes.
- Existing documents only: the CLI cannot create a document.
- Synchronous only: a program returning a promise is rejected.
- Trusted code: programs run with the Node process's full capabilities.

## Permissions

`ir`, `help`, and `guide` evaluate no user code and are safe to allow without
approval. `query` and `exec` evaluate arbitrary JavaScript; request approval for
them unless the user has explicitly accepted that allowing them is equivalent to
allowing arbitrary local Node execution.
