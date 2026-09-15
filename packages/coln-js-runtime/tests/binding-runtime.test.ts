// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import flir from "../../coln-flir-rs/tests/data/GraphRealm.json" with { type: "json" };
import type { createRealm } from "../../coln-compiler/typescript-model/GraphRealm.ts";
import * as runtime from "../src/ts/index.ts";

async function loadGraphRealm(): Promise<ReturnType<typeof createRealm>> {
  const source = await readFile(
    new URL(
      "../../coln-compiler/typescript-model/GraphRealm.ts",
      import.meta.url,
    ),
    "utf8",
  );
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText;
  assert.doesNotMatch(output, /@coln-project\/runtime/);
  const generated = (await import(
    `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`
  )) as { createRealm: typeof createRealm };
  return generated.createRealm(runtime);
}

test("generated realm uses an externally owned managed store", async () => {
  const GraphRealm = await loadGraphRealm();
  let store = runtime.StoreHandle.fromTheory(JSON.stringify(flir));
  const change = new runtime.ManagedStore(
    runtime.managedTransactionBackend(store),
  );
  const graph = new GraphRealm(change);
  const from = graph.root.V.add();
  const to = graph.root.V.add();
  const edge = graph.root.E(from)(to).add();
  assert.throws(() => from.asWire(), /not been committed/);
  if (false) {
    // @ts-expect-error An edge row ID cannot be used where a vertex is required.
    graph.root.E(edge)(to);
  }

  store = change.commit();

  assert.equal(from.asWire().counter, 0);
  assert.equal(to.asWire().counter, 1);
  assert.equal(edge.asWire().counter, 2);

  const view = new GraphRealm(
    new runtime.ManagedStore(runtime.managedReadBackend(store)),
  );
  assert.equal(view.root.V.values().length, 2);
  assert.equal(view.root.E(from)(to).values().length, 1);
  assert.equal(view.root.V.contains(from), true);
});

test("aborting a managed store invalidates branded row IDs", async () => {
  const GraphRealm = await loadGraphRealm();
  let store = runtime.StoreHandle.fromTheory(JSON.stringify(flir));
  const change = new runtime.ManagedStore(
    runtime.managedTransactionBackend(store),
  );
  const graph = new GraphRealm(change);
  const vertex = graph.root.V.add();

  store = change.abort();

  assert.throws(() => vertex.asTxnWire(), /aborted transaction/);
  assert.equal(
    new GraphRealm(new runtime.ManagedStore(runtime.managedReadBackend(store))).root
      .V.values().length,
    0,
  );
});
