// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionMultiArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/function-multi-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedImplicitOutputFailure = {
  expectFailure: {
    label: "a multi-argument function into an inhabited proposition is not synthesized",
    match: /\.f\.total/,
  },
};

const expectedParameterFailure = {
  expectFailure: {
    label: "a function treats equal proof parameters as distinct",
    match: /\.f\.total/,
  },
};

const expectedOutputFailure = {
  expectFailure: {
    label: "a proof-valued function can return a noncanonical proof",
    match: /false !== true/,
  },
};

test("function-multi-argument", () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  const value = realm.root.X.add();
  const inputProof = realm.root.P.add();
  const outputProof = realm.root.Q.add();
  realm.root.f(inputProof)(value).set(outputProof);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(inputProof)(value).get(), outputProof), true);
});

test("function-multi-argument rejects parameters in the wrong order", () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  const value = realm.root.X.add();
  const inputProof = realm.root.P.add();
  const outputProof = realm.root.Q.add();
  realm.root.f(value)(inputProof).set(outputProof);

  assert.throws(() => realm.commit(), /\.f\.foreignKey/);
});

test("function-multi-argument is vacuous when its proof domain is empty", () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  realm.root.X.add();
  const view = realm.commit();

  assert.equal(view.P.values().next().done, true);
  assert.equal(view.Q.values().next().done, true);
});

test("function-multi-argument requires an output when its codomain is uninhabited", () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  realm.root.X.add();
  realm.root.P.add();

  assert.throws(() => realm.commit(), /\.f\.total/);
});

test("function-multi-argument infers its output from an inhabited codomain", expectedImplicitOutputFailure, () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  const value = realm.root.X.add();
  const inputProof = realm.root.P.add();
  const outputProof = realm.root.Q.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.f(inputProof)(value).get(), outputProof), true);
});

test("function-multi-argument ignores proof identity in its parameter", expectedParameterFailure, () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  const value = realm.root.X.add();
  const firstInputProof = realm.root.P.add();
  const secondInputProof = realm.root.P.add();
  const outputProof = realm.root.Q.add();
  realm.root.f(firstInputProof)(value).set(outputProof);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(secondInputProof)(value).get(), outputProof), true);
});

test("function-multi-argument returns a canonical proof", expectedOutputFailure, () => {
  const realm = beginRealm(FunctionMultiArgumentRealm);
  const value = realm.root.X.add();
  const inputProof = realm.root.P.add();
  const firstOutputProof = realm.root.Q.add();
  const secondOutputProof = realm.root.Q.add();
  realm.root.f(inputProof)(value).set(firstOutputProof);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(inputProof)(value).get(), secondOutputProof), true);
});
