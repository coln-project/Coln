// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionMixedPropSetRealm from "../../../coln-compiler/test/golden/basic-ir/function-mixed-prop-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedProofKeyFailure = {
  expectFailure: {
    label: "proof arguments remain part of material function keys",
    match: /3 !== 2/,
  },
};

test("function-mixed-prop-set", () => {
  const realm = beginRealm(FunctionMixedPropSetRealm);
  const input = realm.root.X.add();
  const proof = realm.root.P.add();
  const output = realm.root.Y.add();
  realm.root.choose(input)(proof).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.choose(input)(proof).get(), output), true);
});

test("function-mixed-prop-set retains its erased proof premise", () => {
  const realm = beginRealm(FunctionMixedPropSetRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.choose(input)(input).set(output);

  assert.throws(() => realm.commit(), /\.choose\.foreignKey/);
});

test(
  "function-mixed-prop-set erases its proof argument column",
  expectedProofKeyFailure,
  () => {
    const choose = FunctionMixedPropSetRealm.schema.entities.find(
      (entity) => entity.path.flat().join(".") === "TRealm.choose",
    );

    assert.equal(choose?.value.columns.length, 2);
  },
);
