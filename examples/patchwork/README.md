<!--
SPDX-FileCopyrightText: 2026 Coln contributors
SPDX-License-Identifier: Apache-2.0 OR MIT
-->

# Coln in Patchwork

Coln Lab's three tools, as [Patchwork](https://github.com/inkandswitch/patchwork)
plugins: a Theory Editor, a Store Editor and the Graph Demo. One package, six
plugins — a datatype and a tool for each.

| Tool | Patchwork document | Coln store |
| --- | --- | --- |
| Theory Editor (`coln-theory`) | the theory itself: title, source text, the stores compiled from it | none |
| Store Editor (`coln-store`) | a pointer: title + `coln:` url | the store it points at |
| Graph Demo (`coln-graph`) | a pointer: title + `coln:` url | a store using `src/graph/graph.coln` |

## Two repos, on purpose

A theory is an ordinary Patchwork document. `source` is collaborative text
edited through `@automerge/automerge-codemirror`, so a theory syncs, forks,
versions and takes comments like anything else in Patchwork, and the Theory
Editor never touches Coln's repo until you ask it to make a store.

A Coln store cannot be a Patchwork document. It is not automerge data at all:
it is a Rust store whose commits ride the Subduction sedimentree, registered
with automerge-repo as a custom document type — `defineDocumentType`,
`urlScheme: "coln"`. Neither of those exists in any published automerge-repo
(checked as far as `2.6.0-subduction.48`); they are additions in the fork Coln
depends on. So `window.repo` cannot open a `coln:` url, and this package
bundles the fork and stands up a second `Repo` of its own, pointed at Coln's
relay (`src/coln/repo.ts`).

What the two repos *share* is the wasm underneath. Patchwork initialises
automerge and Subduction on the main thread at exactly the versions Coln pins —
automerge 3.3.2, subduction 0.16.1 — and `vite.config.ts` keeps both external,
so Coln's repo reuses those instances rather than loading its own. What is
bundled is the fork's JavaScript, the Coln store runtime (`runtime.wasm`, about
1MB) and the Haskell compiler (`coln.wasm`, about 4.4MB).

Documents never cross between the two repos. A Patchwork handle stays on the
Patchwork side and a store handle stays on the Coln side; `src/patchwork.ts`
types the host boundary by hand so the compiler enforces that.

### The pointer documents

Patchwork addresses documents by `automerge:` url, so each store gets a
one-field Patchwork document — `{ title, storeUrl, realmName? }` — to give it
an identity Patchwork can list, name, link and share while the rows live in the
Coln repo. Creating a store in the Theory Editor makes both: the store, flushed
to the relay before it is linked, and its pointer. Theories written by Coln Lab
have no pointer for their stores; the Theory Editor makes one the first time you
open such a store.

A pointer made from a theory also records that theory's url, so the Store
Editor offers the trip back — Coln Lab did that with a `?theory=` query
parameter, which Patchwork has no place for.

Copying a pointer does not fork the store behind it. Both copies point at the
same Coln store, which is shared by url.

## Build

The compiler and the store runtime are built out of this repository, so from
the repository root:

```console
just examples/build-web-patchwork   # compiler wasm (if needed), then the bundle
just examples/check-web-patchwork   # tests and typecheck
```

or, once `@coln-project/runtime`, `@coln-project/repo` and the compiler are
built (see `.github/workflows/web-demos.yml` for that sequence):

```console
pnpm --dir examples/patchwork install
pnpm --dir examples/patchwork build     # stages the compiler, then vite
pnpm --dir examples/patchwork test
pnpm --dir examples/patchwork check
```

Installing needs pnpm 11 (`nix develop .#web` has it); pnpm 10 cannot prepare
the git-hosted automerge-repo fork.

Publishing into a Patchwork instance is `pushwork sync` from this directory
(`pnpm push` builds first). `.pushworkattributes` marks `coln.wasm` as an
`artifact` so it is stored as an immutable blob rather than a CRDT document,
and `.pushworkignore` keeps the staging copy of the compiler out of the sync.

## Two knobs, per browser

Both are read once, so changing either takes a reload.

```js
// Which Subduction relay Coln stores sync through.
// Default: wss://coln.sync.inkandswitch.com
localStorage["coln-patchwork:sync-endpoint"] = "ws://127.0.0.1:3030"

// Where the Theory Editor loads the compiler from.
// Default: beside this bundle, as shipped.
localStorage["coln-patchwork:compiler-dist"] = "https://example.github.io/coln/dist/"
```

The endpoint override is how you point these tools at a local relay
(`pnpm --dir examples/lab server`). The compiler override loads the wasm from a
deployed Coln Lab instead of shipping it inside the module.

## What Coln Lab has that this does not

- **The router, the home page and the nav.** Patchwork supplies navigation and
  the document url; a tool receives its handle and renders.
- **Theory sync status.** Patchwork owns syncing for theories. Coln stores keep
  their own indicator (`src/coln/sync.svelte.ts`), since Patchwork's knows
  nothing about Coln's relay.
- **The document-load error pages.** Patchwork opens the document; what remains
  is a store failing to load, reported inline.
- **`paneforge` and Tailwind.** Panes are CSS grid, and the styling derives
  every colour from the host's `--editor-*` / `--studio-*` theme instead of
  Coln Lab's fixed dark palette — Tailwind's preflight would have reset
  Patchwork's own chrome, since tools render into the light DOM.
- **The CDN import.** GHC's `loadHaskellWasm.js` fetches its WASI shim from
  jsdelivr; `src/theory/compiler.ts` does that job with the shim bundled.

Carried over unchanged: the compiler driver, the IR reader, the store schema
reader, the value formatter, the REPL sandbox and snapshotter, the graph
projection, and the generated `GraphRealm` bindings.

## Tests

`pnpm test` runs two projects. `unit` covers the browser-shaped code under
browser export conditions. `store` stands up a real Coln store — forked repo,
store runtime, generated bindings, `graph.ts` — and checks that vertices and
directed edges come back out; it runs under Node, where each wasm module can
initialise itself (`tests/setup-wasm.ts`).

`node probe/compiler-probe.mjs` (after a build) loads the **built** compiler
chunk in headless Chrome and compiles a theory with it. That is the one part of
this package a unit test cannot reach, and it is where the port's first bug
lived: the compiler resolved its two files against `import.meta.url`, which
points at `dist/assets/` for a chunk, while the files were staged to `dist/`.
Both are now `?url` imports, so vite emits them beside the chunk and writes the
urls. Last run:

```
import chunk             25.2 ms
loadCompiler             83.9 ms   ← fetch + instantiate 4.4MB of wasm
compile graph theory     16.0 ms   0 diagnostics, 1 realm
compile broken theory     5.5 ms   1 diagnostic (E0314)
recompile graph theory    2.3 ms   identical IR
```
