# Coln CLI Demo

`coln-repo` is an experimental, JSON-only CLI for agents to read and update an
existing synchronized Coln document. It exposes the document's compiled IR
without interpreting its domain and evaluates trusted, synchronous JavaScript
for reads and transactions.

Nothing in this protocol is stable yet.

## Build

Build local packages first:

```bash
npm ci --prefix packages/coln-js-runtime
npm run --prefix packages/coln-js-runtime build
pnpm --dir packages/coln-repo install
pnpm --dir packages/coln-repo build
pnpm --dir examples/cli-demo install
pnpm --dir examples/cli-demo build
```

The built binary is `examples/cli-demo/dist/cli.js`, exposed as `coln-repo` by
the package manifest. Put it on your `PATH` with:

```bash
pnpm --dir examples/cli-demo link --global
```

The examples below assume `coln-repo` is on `PATH`; otherwise substitute
`node examples/cli-demo/dist/cli.js`.

## Agent Skill

The model-facing workflow lives in [`skills/coln-repo/SKILL.md`](./skills/coln-repo/SKILL.md),
a portable skill (frontmatter plus markdown) that OpenCode, Claude Code, and
similar harnesses can load. Two ways to reach it:

```bash
coln-repo guide                          # print the skill body as JSON
coln-repo install-skill                  # copy to ~/.agents/skills/coln-repo
coln-repo install-skill --dir ~/.claude/skills
```

`install-skill` reports `installed`, `updated`, or `unchanged`; rerun it after
upgrading the CLI to refresh the installed copy. No harness configuration or
permission rules are shipped; see Security below.

## Commands

Every command takes the document as a named flag. `--endpoint` overrides
`SUBDUCTION_ENDPOINT`; otherwise the public Subduction relay is used.

```bash
coln-repo ir --document <automerge-url> [--endpoint <ws-url>]
```

`ir` returns the compiled JSON IR unchanged inside the response's `ir` field.
Agents should use it as structural evidence, not domain documentation.

`query` reads one JavaScript expression from stdin. A read-only `store` is in
scope with `jsonIR()`, `scanTable(path)`, `rowById(path, rowRef)`, and `heads()`.

```bash
coln-repo query --document automerge:... <<'JS'
store.scanTable("Records.Documents")
JS
```

`exec` reads a JavaScript function body from stdin. The current transaction API
is in scope as `txn`; today it exposes `txn.add(path, values)`. Return a
JSON-serializable value to include it in the response.

```bash
coln-repo exec --document automerge:... <<'JS'
const folder = txn.add("Records.Folders", [{ tag: "string", value: "Inbox" }])
const document = txn.add("Records.Documents", [
  folder,
  { tag: "string", value: "Notes" },
])
return { folder, document }
JS
```

Values passed to `txn.add` are tagged:

```js
{ tag: "string", value: "hello" }
{ tag: "int", value: 42 }
{ tag: "row_id", value: rowReference }
```

`txn.add` returns a complete tagged `row_id` value. Pass that value directly to
later `txn.add` calls; do not wrap it in another `{ tag: "row_id", ... }`.

Programs must complete synchronously. Their return values must be JSON
serializable. Program `console` output is redirected to stderr so stdout remains
one JSON response.

## Responses

Every response contains `protocolVersion: 0`, `ok`, and the command where known.
Successful writes include heads before and after the local commit plus sync
status. Errors identify their phase and whether a mutation was applied locally.

| Exit | Code | Phase | Mutation state |
|---|---|---|---|
| 2 | `INVALID_ARGUMENTS`, `PROGRAM_REQUIRED` | `arguments` | `not_applied` |
| 3 | `DOCUMENT_OPEN_FAILED` | `open` | `not_applied` |
| 4 | `IR_FAILED`, `QUERY_FAILED` | `read`, `evaluate` | `not_applied` |
| 4 | `GUIDE_FAILED`, `SKILL_INSTALL_FAILED` | `read`, `install` | `not_applied` |
| 5 | `TRANSACTION_FAILED` | `transaction` | `not_applied` |
| 5 | `RESULT_INVALID` | `result` | `not_applied` |
| 5 | `RESULT_FAILED` | `result` | `applied_locally` |
| 6 | `SYNC_FAILED` | `flush` | `applied_locally` |
| 1 | `INTERNAL_ERROR` | `internal` | `unknown` |

`RESULT_INVALID` means the program's return value was not JSON; the transaction
was rolled back so the caller never loses the row references it asked for.

If an `exec` response reports `mutationState: "applied_locally"`, do not retry it
automatically: the local commit succeeded, but relay synchronization or result
reporting failed afterwards. The CLI does not judge retryability of
`not_applied` failures; connection timeouts may be transient, schema rejections
are not.

## Security

`query` and `exec` evaluate trusted JavaScript with this Node process's
capabilities. Agent harnesses should request approval for them by default.
Allowing either command without approval is equivalent to allowing arbitrary
local Node execution. `ir`, `guide`, and `install-skill` do not evaluate user
JavaScript and may be allowlisted separately.

## Verify

```bash
pnpm --dir examples/cli-demo test
```
