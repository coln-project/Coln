// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProjectionRealm from "../../../coln-compiler/test/golden/basic-ir/projection.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("projection", expectedFailure, () => {
  const realm = beginRealm(ProjectionRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const firstValue = realm.root.E(first).add();
  const secondValue = realm.root.E(second).add();
  realm.root.r({ first, second: first }).set(firstValue);
  realm.root.r({ first, second }).set(secondValue);
  realm.root.r({ first: second, second: first }).set(firstValue);
  realm.root.r({ first: second, second }).set(secondValue);
  const view = realm.commit();

  assert.equal(view.X.has(first), true);
  assert.equal(view.X.has(second), true);
  assert.equal(view.E(first).has(firstValue), true);
  assert.equal(view.E(second).has(secondValue), true);
  assert.equal(valueEqual(view.r({ first, second }).get(), secondValue), true);
});

test("projection rejects a value at a different projected value", expectedFailure, () => {
  const realm = beginRealm(ProjectionRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const firstValue = realm.root.E(first).add();
  const secondValue = realm.root.E(second).add();
  realm.root.r({ first, second: first }).set(firstValue);
  realm.root.r({ first, second }).set(firstValue);
  realm.root.r({ first: second, second: first }).set(firstValue);
  realm.root.r({ first: second, second }).set(secondValue);

  assert.throws(() => realm.commit(), /\.r\.foreignKey/);
});
