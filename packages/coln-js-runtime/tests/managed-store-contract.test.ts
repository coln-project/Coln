// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";
import {
  ManagedStore,
  type ManagedStoreBackend,
  type TxnWireTuple,
  type WhereClause,
} from "../src/ts/index.ts";

const query: WhereClause = {
  table_name: "view.items",
  row_id: null,
  values: ["bound"],
};
const rowId = { commit: "existing", counter: 2 };
const unavailableReads = {
  all_proj() {
    throw new Error("unexpected all_proj");
  },
  all_row_id() {
    throw new Error("unexpected all_row_id");
  },
  one_proj() {
    throw new Error("unexpected one_proj");
  },
  exists() {
    throw new Error("unexpected exists");
  },
};
const unavailableChanges = {
  add() {
    throw new Error("unexpected add");
  },
  commit() {
    throw new Error("unexpected commit");
  },
  abort() {
    throw new Error("unexpected abort");
  },
};

test("delegates relation reads through a structural backend", () => {
  const calls: string[] = [];
  const backend: ManagedStoreBackend<never> = {
    ...unavailableChanges,
    all_proj(received, select) {
      assert.deepEqual(received, query);
      assert.deepEqual(select, [1]);
      calls.push("all_proj");
      return [["projected"]];
    },
    all_row_id(received) {
      assert.deepEqual(received, query);
      calls.push("all_row_id");
      return [rowId];
    },
    one_proj(received, select) {
      assert.deepEqual(received, query);
      assert.deepEqual(select, [0]);
      calls.push("one_proj");
      return [7];
    },
    exists(received) {
      assert.deepEqual(received, query);
      calls.push("exists");
      return true;
    },
  };
  const store = new ManagedStore(backend);

  assert.deepEqual(store.all_proj(query, [1]), [["projected"]]);
  assert.deepEqual(store.all_row_id(query), [rowId]);
  assert.deepEqual(store.one_proj(query, [0]), [7]);
  assert.equal(store.exists(query), true);
  assert.deepEqual(calls, ["all_proj", "all_row_id", "one_proj", "exists"]);
});

test("finalizes pending row IDs through a structural transaction backend", () => {
  const recoveredStore = { backend: "fake" };
  const additions: Array<{
    path: string;
    values: TxnWireTuple;
  }> = [];
  let counter = 0;
  const backend: ManagedStoreBackend<typeof recoveredStore> = {
    ...unavailableReads,
    add(path, values) {
      additions.push({ path, values });
      return { type: "Pending", value: { txId: 4, counter: counter++ } };
    },
    commit() {
      return { commit: "committed", store: recoveredStore };
    },
    abort() {
      throw new Error("unexpected abort");
    },
  };
  const store = new ManagedStore(backend);
  const first = store.add("root.Items", ["one"]);
  const second = store.add("root.Links", [first.asTxnWire(), 2]);

  assert.equal("finish" in first, false);
  assert.equal("abort" in first, false);
  assert.deepEqual(first.asTxnWire(), {
    type: "Pending",
    value: { txId: 4, counter: 0 },
  });
  assert.deepEqual(additions, [
    { path: "root.Items", values: ["one"] },
    {
      path: "root.Links",
      values: [
        { type: "Pending", value: { txId: 4, counter: 0 } },
        2,
      ],
    },
  ]);
  assert.strictEqual(store.commit(), recoveredStore);
  assert.deepEqual(first.asWire(), { commit: "committed", counter: 0 });
  assert.deepEqual(second.asWire(), { commit: "committed", counter: 1 });
});

test("abort recovers ownership and invalidates pending row IDs", () => {
  const recoveredStore = { backend: "fake" };
  const backend: ManagedStoreBackend<typeof recoveredStore> = {
    ...unavailableReads,
    add() {
      return { type: "Pending", value: { txId: 9, counter: 0 } };
    },
    commit() {
      throw new Error("unexpected commit");
    },
    abort() {
      return recoveredStore;
    },
  };
  const store = new ManagedStore(backend);
  const row = store.add("root.Items", []);

  assert.strictEqual(store.abort(), recoveredStore);
  assert.throws(() => row.asTxnWire(), /aborted transaction/);
});

test("a failed commit leaves the backend available for recovery", () => {
  const recoveredStore = { backend: "fake" };
  const backend: ManagedStoreBackend<typeof recoveredStore> = {
    ...unavailableReads,
    add() {
      return { type: "Pending", value: { txId: 3, counter: 0 } };
    },
    commit() {
      throw new Error("commit failed");
    },
    abort() {
      return recoveredStore;
    },
  };
  const store = new ManagedStore(backend);
  const row = store.add("root.Items", []);

  assert.throws(() => store.commit(), /commit failed/);
  assert.strictEqual(store.abort(), recoveredStore);
  assert.throws(() => row.asTxnWire(), /aborted transaction/);
});
