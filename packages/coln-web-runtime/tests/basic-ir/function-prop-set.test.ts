// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionPropSetRealm from "../../../coln-compiler/test/golden/basic-ir/function-prop-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedProofIrrelevanceFailure = {
  expectFailure: {
    label: "a function treats equal proof inputs as distinct",
    match: /\.next\.total/,
  },
};

test("function-prop-set", () => {
  const realm = beginRealm(FunctionPropSetRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-prop-set requires an output for every input", () => {
  const realm = beginRealm(FunctionPropSetRealm);
  realm.root.X.add();

  assert.throws(() => realm.commit(), /\.next\.total/);
});

test("function-prop-set rejects an output from the wrong table", () => {
  const realm = beginRealm(FunctionPropSetRealm);
  const input = realm.root.X.add();
  const wrongOutput = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(wrongOutput);
  realm.root.next(wrongOutput).set(output);

  assert.throws(() => realm.commit(), /\.next\.foreignKey/);
});

test("function-prop-set ignores proof identity in its input", expectedProofIrrelevanceFailure, () => {
  const realm = beginRealm(FunctionPropSetRealm);
  const firstInput = realm.root.X.add();
  const secondInput = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(firstInput).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(secondInput).get(), output), true);
});
