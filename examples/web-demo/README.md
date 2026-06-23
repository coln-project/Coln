# Coln + automerge-repo web demo

This demo stores a compiler-generated Coln realm in `automerge-repo` using a
custom document type. Custom document types aren't actually a thing in 
`automerge-repo` (yet), so we use a vendored version of `automerge-repo` from
the `doctypes` branch which adds support for custom document types.

We define a function which takes a compiled FFI bindings from compiler
and turns it into an automerge-repo document type. This is the `colnDocType`
function.

Then, we can create and find handles containing these documents, modify and
render them, and synchronize them using the public subduction sycn server.
That's what most of the code here does.

## Compiling the schema

The demo source schema is:

- `graph.coln`

The generated FFI/IR artifacts live in:

- `src/generated/Graph.ts`
- `src/generated/GraphRealm.ts`
- `src/generated/GraphRealm.json`

`GraphRealm.ts` exports the generated `View`, `Transaction`, and `schema`. The
generic `src/colnDocType.ts` consumes that compiled FFI directly:

```ts
const coln = colnDocType(GraphRealm)
```

`GraphRealm.json` is passed directly to `StoreHandle.fromTheory(...)`; no local
IR-to-FlatTheory translation is needed anymore.

You can compile them by running `./compile.sh`

## Run

From `coln/examples/web-demo/`:

```bash
pnpm install
pnpm dev
```

Open the Vite URL, then open the hash URL shown in the page in another tab. Each
tab creates its own Repo and syncs via Subduction.

## Build

```bash
pnpm build
```

The build script runs:

1. `./compile.sh` to regenerate `src/generated/*` from `graph.coln`
2. `tsc --noEmit`
3. `vite build`

## Test

```bash
pnpm exec playwright install chromium # first time only
pnpm test:e2e
```
