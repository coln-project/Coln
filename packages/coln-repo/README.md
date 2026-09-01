# `@coln-project/repo`

Coln Repo exposes Coln stores as [Automerge Repo](https://github.com/automerge/automerge-repo) document handles. Use them to create or find stores, read the current typed view, make atomic changes, and subscribe to changes to update application state or UI. Automerge Repo provides persistence and synchronization.

Coln Repo currently requires the experimental [doctypes](https://github.com/automerge/automerge-repo/tree/doctypes) branch.

## Create

Create a Coln store using generated bindings.

```ts
import * as Bindings from "./ExampleRealm.js";
import { create } from "@coln-project/repo";

const handle = create(repo, Bindings);
```

## Find

Find a Coln store by Automerge URL. Bindings are optional and must match the document schema when supplied.

```ts
import * as Bindings from "./ExampleRealm.js";
import { find } from "@coln-project/repo";

const handle = await find(repo, url, Bindings);
```

Apply bindings to a raw handle later if needed:

```ts
import * as Bindings from "./ExampleRealm.js";
import { applyBindings, find } from "@coln-project/repo";

const rawHandle = await find(repo, url);
const handle = applyBindings(rawHandle, Bindings);
```

> [!IMPORTANT]
> Once bindings are applied, later raw lookups in the same Repo may return that bound handle. It retains the raw Coln operations and adds `root` to documents and change transactions.

## Read

Read the current document synchronously or subscribe to changes.

```ts
const doc = handle.doc();
doc.scanTable("Example.Items");
doc.root.Items.values();

handle.on("change", ({ doc }) => {
  doc?.scanTable("Example.Items");
  doc?.root.Items.values();
});
```

Documents expose `heads()`, `jsonIR()`, `rowById()`, and `scanTable()`.

## Write

Changes are synchronous and atomic. A thrown error aborts the transaction.

```ts
handle.change((tx) => {
  tx.root.Items.add();
});
```

Transactions expose `add(path, values)`; bound transactions also expose `root`.

The underlying `StoreHandle` is available through `handle.fullDoc().store` for advanced use. Its lifecycle operations bypass Coln Repo changes, events, and synchronization; prefer `doc()` and `change()`.

## Current Limitations

Values returned by `handle.doc()` are current documents, not durable snapshots.

Generated read methods such as `has()` and `values()` are not supported inside `handle.change()`. Typed transactions currently support writes only.
