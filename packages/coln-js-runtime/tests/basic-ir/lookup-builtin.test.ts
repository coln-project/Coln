// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupBuiltinRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-builtin.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-builtin", () => {
  const realm = beginRealm(LookupBuiltinRealm);
  const source = realm.root.X.add();
  const rank = { tag: "int", value: 1 } as const;
  const edge = realm.root.E(rank).add();
  realm.root.rank(source).set(rank);
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.X.has(source), true);
  assert.equal(view.E(rank).has(edge), true);
  assert.equal(valueEqual(view.rank(source).get(), rank), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});

test("lookup-builtin rejects an edge at a different rank", () => {
  const realm = beginRealm(LookupBuiltinRealm);
  const source = realm.root.X.add();
  const rank = { tag: "int", value: 1 } as const;
  const otherRank = { tag: "int", value: 2 } as const;
  const edge = realm.root.E(otherRank).add();
  realm.root.rank(source).set(rank);
  realm.root.edge(source).set(edge);

  assert.throws(() => realm.commit(), /\.edge\.foreignKey/);
});
