// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import type { Value } from "@coln-project/runtime";
import { applyBindings, colnDocType, create, find, type ColnDocument } from "../src/index";
import * as itemFfi from "./fixtures/itemFfi";
import schema from "./generated/GraphRealm.json" with { type: "json" };
import * as GraphRealm from "./generated/GraphRealm";
import { createTestRepoPair, waitForChange, type TestRepoPair } from "./helpers/TestRepoPair";

let repos: TestRepoPair;

beforeEach(async () => {
  repos = await createTestRepoPair();
});

afterEach(async () => {
  await repos.shutdown();
});

test("create returns a bound, initialized handle", () => {
  const handle = create(repos.source, GraphRealm);
  const document = handle.doc();

  assert.deepEqual(Object.keys(document).sort(), ["heads", "jsonIR", "root", "rowById", "scanTable"]);
  assert.deepEqual(JSON.parse(document.jsonIR()), schema);
  assert.deepEqual(document.heads(), handle.rawHeads());
  assert.deepEqual(document.scanTable("GraphRealm.V"), []);
});

test("document can be updated and changes are reflected", () => {
  const handle = create(repos.source, GraphRealm);

  let v1!: Value, v2!: Value, e1!: Value;

  handle.change((tx) => {
    v1 = tx.root.V.add();
    v2 = tx.root.V.add();
    e1 = tx.root.E(v1)(v2).add();
  });

  const doc = handle.doc();

  assert(doc.root.V.has(v1));
  assert(doc.root.V.has(v2));
  assert(doc.root.E(v1)(v2).has(e1));
  assert.equal(v1.tag, "row_id");
  assert.deepEqual(doc.rowById("GraphRealm.V", v1.value)?.rowId, v1);
});

test("changes expose only safe Coln operations", () => {
  const handle = create(repos.source, GraphRealm);

  handle.change(transaction => {
    assert.deepEqual(Object.keys(transaction).sort(), ["add", "root"]);
    transaction.add("GraphRealm.V", []);
  });

  assert.equal(handle.doc().scanTable("GraphRealm.V").length, 1);
});

test("failed changes are aborted and leave the handle usable", () => {
  const handle = create(repos.source, GraphRealm);
  let committed!: Value;

  assert.throws(
    () =>
      handle.change((transaction) => {
        transaction.root.V.add();
        throw new Error("abort");
      }),
    new Error("abort"),
  );
  handle.change((transaction) => {
    committed = transaction.root.V.add();
  });

  assert.deepEqual(
    handle
      .doc()
      .scanTable("GraphRealm.V")
      .map((row) => row.rowId),
    [committed],
  );
});

test("typed handles sync changes", async () => {
  const handle = create(repos.source, GraphRealm);
  const replica = await find(repos.replica, handle.url, GraphRealm);
  let vertex!: Value;

  const changed = new Promise<ColnDocument<typeof GraphRealm>>(resolve => {
    replica.once("change", payload => {
      if (payload.doc) resolve(payload.doc);
    });
  });
  handle.change((tx) => {
    vertex = tx.root.V.add();
  });
  const changedDocument = await changed;

  assert(changedDocument.root.V.has(vertex));
  assert.deepEqual(changedDocument.scanTable("GraphRealm.V").map(row => row.rowId), [vertex]);
  assert(replica.doc().root.V.has(vertex));
});

test("raw changes sync into a typed handle", async () => {
  const handle = create(repos.source, GraphRealm);
  const replica = await repos.replica.find(handle.url, colnDocType);
  let vertex!: Value;

  const changed = waitForChange(handle);
  replica.change((transaction) => {
    vertex = transaction.add("GraphRealm.V", []);
  });
  await changed;

  assert(handle.doc().root.V.has(vertex));
});

test("find returns a raw handle when bindings are omitted", async () => {
  const handle = create(repos.source, GraphRealm);
  const replica = await find(repos.replica, handle.url);

  assert.deepEqual(Object.keys(replica.doc()).sort(), ["heads", "jsonIR", "rowById", "scanTable"]);
});

test("bindings can be applied to a raw found handle later", async () => {
  const handle = create(repos.source, GraphRealm);
  const rawReplica = await find(repos.replica, handle.url);
  const replica = applyBindings(rawReplica, GraphRealm);

  assert.strictEqual(replica, rawReplica);
  assert.strictEqual(applyBindings(replica, GraphRealm), replica);

  let vertex!: Value;
  replica.change(transaction => {
    vertex = transaction.root.V.add();
  });

  assert(replica.doc().root.V.has(vertex));
  assert.deepEqual(replica.doc().scanTable("GraphRealm.V").map(row => row.rowId), [vertex]);
});

test("applying different bindings to a bound handle is rejected", () => {
  const handle = create(repos.source, GraphRealm);
  const otherBindings = { ...GraphRealm };

  assert.throws(
    () => applyBindings(handle, otherBindings),
    new TypeError("Coln handle already uses different realm bindings"),
  );
});

test("applying bindings for a different schema is rejected", () => {
  const handle = repos.source.create(schema, colnDocType);

  assert.throws(
    () => applyBindings(handle, itemFfi),
    new TypeError("Realm bindings schema does not match Coln document schema"),
  );
});
