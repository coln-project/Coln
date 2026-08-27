// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/function-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedProofIrrelevanceFailure = {
  expectFailure: {
    label: "a function can return distinct proofs of the same proposition",
    match: /false !== true/,
  },
};

const expectedImplicitOutputFailure = {
  expectFailure: {
    label: "a function into an inhabited proposition is not synthesized",
    match: /\.next\.total/,
  },
};

test("function-prop-prop is vacuous when its domain is empty", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const view = realm.commit();

  assert.equal(view.X.values().next().done, true);
  assert.equal(view.Y.values().next().done, true);
});

test("function-prop-prop", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-prop-prop requires an output when its codomain is uninhabited", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  realm.root.X.add();

  assert.throws(() => realm.commit(), /\.next\.total/);
});

test("function-prop-prop infers its output from an inhabited codomain", expectedImplicitOutputFailure, () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-prop-prop rejects an output from the wrong table", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const input = realm.root.X.add();
  const wrongOutput = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(wrongOutput);
  realm.root.next(wrongOutput).set(output);

  assert.throws(() => realm.commit(), /\.next\.foreignKey/);
});

test("function-prop-prop returns equal proofs for equal proof inputs", expectedProofIrrelevanceFailure, () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const firstInput = realm.root.X.add();
  const secondInput = realm.root.X.add();
  const firstOutput = realm.root.Y.add();
  const secondOutput = realm.root.Y.add();
  realm.root.next(firstInput).set(firstOutput);
  realm.root.next(secondInput).set(secondOutput);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(firstInput).get(), view.next(secondInput).get()), true);
});
