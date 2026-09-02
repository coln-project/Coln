# Coln CLI demo

`coln-repo` is a scriptable CLI for reading and updating an existing synchronized
Coln document. Unlike `examples/sync`, it has no fixed schema or interactive UI.

Build local packages first:

```bash
npm ci --prefix packages/coln-js-runtime
npm run --prefix packages/coln-js-runtime build
pnpm --dir packages/coln-repo install
pnpm --dir packages/coln-repo build
pnpm --dir examples/cli-demo install
pnpm --dir examples/cli-demo build
```

Run it with an Automerge URL:

```bash
node examples/cli-demo/dist/cli.js <automerge-url> ir
node examples/cli-demo/dist/cli.js <automerge-url> query 'store.scanTable("GraphRealm.V")'
node examples/cli-demo/dist/cli.js <automerge-url> exec 'txn.add("GraphRealm.V", [])'
```

Use `-` as the query or script argument to read from stdin. `query` receives a
read-only `store`; `exec` receives an add-only `txn` and waits for the change to
sync before exiting. Set `SUBDUCTION_ENDPOINT` to use another WebSocket relay.

JavaScript is trusted local code and must complete synchronously.

Verify with:

```bash
pnpm --dir examples/cli-demo test
```
