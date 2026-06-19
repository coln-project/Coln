import assert from "node:assert/strict";
import test from "node:test";

import { StoreHandle } from "#wasm-bodge/bindings";

import graphtheory from "./GraphRealm.json" with { type: "json" };

test("resolve pending row id to existing on commit", () => {
  const store = StoreHandle.fromTheory(JSON.stringify(graphtheory));
  let txn = store.beginTransaction();
  let vertex = txn.add("GraphRealm.V", []);

  assert.ok("pending" in vertex.value, "is pending");

  const res = txn.commit();

  assert.ok("existing" in vertex.value, "resolved to existing after commit");
  assert.equal(vertex.tag, "row_id", "tag unchanged");
  assert.equal(
    typeof vertex.value.existing.commit,
    "string",
    "commit is hex string",
  );
  assert.equal(
    vertex.value.existing.commit.length,
    64,
    "32-byte hash hex with 64 hex chars",
  );
  assert.equal(typeof vertex.value.existing.counter, "number");

  const store2 = res.takeStore();
  const rows = store2.scanTable("GraphRealm.V");

  assert.equal(rows.length, 1);
});
