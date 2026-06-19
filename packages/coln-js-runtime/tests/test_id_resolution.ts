import assert from "node:assert/strict";
import test from "node:test";

import { StoreHandle } from "#wasm-bodge/bindings";

import graphtheory from "./graph.json" with { type: "json" };

test("resolve pending row id to existing on commit", () => {
  const store = StoreHandle.fromTheory(JSON.stringify(graphtheory));
  let txn = store.beginTransaction();
  let gg = txn.add("gog.graphs", []);

  assert.ok("pending" in gg.value, "is pending");

  const res = txn.commit();

  assert.ok("existing" in gg.value, "resolved to existing after commit");
  assert.equal(gg.tag, "row_id", "tag unchanged");
  assert.equal(
    typeof gg.value.existing.commit,
    "string",
    "commit is hex string",
  );
  assert.equal(
    gg.value.existing.commit.length,
    64,
    "32-byte hash hex with 64 hex chars",
  );
  assert.equal(typeof gg.value.existing.counter, "number");

  const store2 = res.takeStore();
  const rows = store2.scanTable("gog.graphs");

  assert.equal(rows.length, 1);
});
