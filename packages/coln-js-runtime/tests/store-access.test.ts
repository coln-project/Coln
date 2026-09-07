// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { StoreHandle, valueEqual } from "#wasm-bodge/bindings";

import theory from "../../coln-compiler/test/golden/graph.ts.output/GraphRealm.json" with { type: "json" };

test("Add vertices and edges to a store", () => {
  let store = StoreHandle.fromTheory(JSON.stringify(theory));
  let txn = store.beginTransaction();

  // adding two vertices
  let v1 = txn.add("GraphRealm.V", []);
  let v2 = txn.add("GraphRealm.V", []);

  // add an edge between them
  let e1 = txn.add("GraphRealm.E", [v1, v2]);

  try {
    store = txn.commit().takeStore();
  } catch (e) {
    store = txn.takeStore();
    throw e;
  }
  let vs = store.scanTable("GraphRealm.V");
  let es = store.scanTable("GraphRealm.E");
  // We have two vertices and one edge
  assert.equal(vs.length, 2);
  assert.equal(es.length, 1);

  txn = store.beginTransaction();
  let v3 = txn.add("GraphRealm.V", []);
  let v4 = txn.add("GraphRealm.V", []);

  let e2 = txn.add("GraphRealm.E", [v3, v4]);
  // Add a second edge between v1 and v2
  let e3 = txn.add("GraphRealm.E", [v1, v2]);

  try {
    store = txn.commit().takeStore();
  } catch (e) {
    store = txn.takeStore();
    throw e;
  }

  // Now find out all vertices connected to e2
  const e2_vs = [];
  es = store.scanTable("GraphRealm.E");
  for (let e of es) {
    if (valueEqual(e.rowId, e2)) {
      e2_vs.push(e.values[0]);
      e2_vs.push(e.values[1]);
    }
  }

  const expected = [v3, v4];
  assert.deepStrictEqual([...e2_vs].sort(), [...expected].sort());

  // Find out all edges between v1 and v2
  const v1v2_edges = [];
  for (let e of es) {
    if (
      (valueEqual(e.values[0], v1) && valueEqual(e.values[1], v2)) ||
      (valueEqual(e.values[0], v2) && valueEqual(e.values[1], v1))
    ) {
      v1v2_edges.push(e.rowId);
    }
  }

  const expected_edges = [e1, e3];
  assert.deepStrictEqual([...v1v2_edges].sort(), [...expected_edges].sort())
});
