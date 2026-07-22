// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupRecordCompositionRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-record-composition.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record fields are not exposed through generated table cells",
    match: /Cannot read properties of undefined \(reading 'set'\)/,
  },
};

test("lookup-record-composition", expectedFailure, () => {
  const realm = beginRealm(LookupRecordCompositionRealm);
  const source = realm.root.X.add();
  const rank = { tag: "int", value: 1 } as const;
  const name = { tag: "string", value: "example" } as const;
  realm.root.key(source).rank.set(rank);
  const slot = realm.root.PayloadAt(rank).add();
  realm.root.slot(source).set(slot);
  realm.root.payload(rank)(slot).name.set(name);
  const edge = realm.root.E(name).add();
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.key(source).rank.get(), rank), true);
  assert.equal(view.PayloadAt(rank).has(slot), true);
  assert.equal(valueEqual(view.slot(source).get(), slot), true);
  assert.equal(valueEqual(view.payload(rank)(slot).name.get(), name), true);
  assert.equal(view.E(name).has(edge), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});
