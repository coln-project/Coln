// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupRecordExpansionRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-record-expansion.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record fields are not exposed through the generated table cell",
    match: /Cannot read properties of undefined \(reading 'set'\)/,
  },
};

test("lookup-record-expansion", expectedFailure, () => {
  const realm = beginRealm(LookupRecordExpansionRealm);
  const source = realm.root.X.add();
  const name = { tag: "string", value: "example" } as const;
  const rank = { tag: "int", value: 1 } as const;
  const payload = { name, rank } as const;
  realm.root.payload(source).name.set(name);
  realm.root.payload(source).rank.set(rank);
  const edge = realm.root.E(payload).add();
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.payload(source).name.get(), name), true);
  assert.equal(valueEqual(view.payload(source).rank.get(), rank), true);
  assert.equal(view.E(payload).has(edge), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});
