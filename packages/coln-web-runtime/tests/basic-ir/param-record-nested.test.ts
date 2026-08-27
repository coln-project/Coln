// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ParamRecordNestedRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-nested.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "nested record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("param-record-nested", expectedFailure, () => {
  const realm = beginRealm(ParamRecordNestedRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const firstFirst = { inner: { value: first }, sibling: first };
  const firstSecond = { inner: { value: first }, sibling: second };
  const secondFirst = { inner: { value: second }, sibling: first };
  const secondSecond = { inner: { value: second }, sibling: second };
  realm.root.selected(firstFirst).set(first);
  realm.root.selected(firstSecond).set(first);
  realm.root.selected(secondFirst).set(second);
  realm.root.selected(secondSecond).set(second);
  const value = realm.root.nested(firstSecond).add();
  const view = realm.commit();

  assert.equal(view.X.has(first), true);
  assert.equal(view.X.has(second), true);
  assert.equal(view.nested(firstSecond).has(value), true);
  assert.equal(valueEqual(view.selected(firstSecond).get(), first), true);
});
