// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupRecordFieldRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-record-field.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record fields are not exposed through the generated table cell",
    match: /Cannot read properties of undefined \(reading 'set'\)/,
  },
};

test("lookup-record-field", expectedFailure, () => {
  const realm = beginRealm(LookupRecordFieldRealm);
  const source = realm.root.X.add();
  const name = { tag: "string", value: "example" } as const;
  const rank = { tag: "int", value: 1 } as const;
  const payload = realm.root.payload(source);
  payload.name.set(name);
  payload.rank.set(rank);
  const edge = realm.root.E(rank).add();
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.X.has(source), true);
  assert.equal(view.E(rank).has(edge), true);
  assert.equal(valueEqual(view.payload(source).name.get(), name), true);
  assert.equal(valueEqual(view.payload(source).rank.get(), rank), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});

test("lookup-record-field rejects an edge at a different payload rank", expectedFailure, () => {
  const realm = beginRealm(LookupRecordFieldRealm);
  const source = realm.root.X.add();
  const name = { tag: "string", value: "example" } as const;
  const rank = { tag: "int", value: 1 } as const;
  const otherRank = { tag: "int", value: 2 } as const;
  const payload = realm.root.payload(source);
  payload.name.set(name);
  payload.rank.set(rank);
  const edge = realm.root.E(otherRank).add();
  realm.root.edge(source).set(edge);

  assert.throws(() => realm.commit(), /\.edge\.foreignKey/);
});
