// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as RecordFieldOrderRealm from "../../../coln-compiler/test/golden/basic-ir/record-field-order.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("record-field-order", expectedFailure, () => {
  const realm = beginRealm(RecordFieldOrderRealm);
  const pair = {
    first: { tag: "int", value: 1 },
    second: { tag: "int", value: 2 },
  } as const;
  const edge = realm.root.E(pair.second)(pair.first).add();
  const related = realm.root.R(pair)(edge).add();
  const view = realm.commit();

  assert.equal(view.E(pair.second)(pair.first).has(edge), true);
  assert.equal(view.R(pair)(edge).has(related), true);
});

test("record-field-order rejects declaration order as dependency order", expectedFailure, () => {
  const realm = beginRealm(RecordFieldOrderRealm);
  const pair = {
    first: { tag: "int", value: 1 },
    second: { tag: "int", value: 2 },
  } as const;
  const edge = realm.root.E(pair.first)(pair.second).add();
  realm.root.R(pair)(edge).add();

  assert.throws(() => realm.commit(), /\.R\.foreignKey/);
});
