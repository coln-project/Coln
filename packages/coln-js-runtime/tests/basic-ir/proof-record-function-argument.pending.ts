// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { proveEquality, valueEqual } from "@coln-project/runtime";
import * as ProofRecordFunctionArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-function-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-function-argument", () => {
  const realm = beginRealm(ProofRecordFunctionArgumentRealm);
  const equal = realm.root.X.add();
  const value = {
    first: equal,
    second: equal,
    proof: proveEquality(equal, equal),
    trailing: equal,
  };
  const accepted = realm.root.Accepted(value).add();
  const view = realm.commit();

  assert.equal(view.Accepted(value).has(accepted), true);
});

test(
  "proof-record-function-argument ignores nested proof identity",
  () => {
    const realm = beginRealm(ProofRecordFunctionArgumentRealm);
    const equal = realm.root.X.add();
    const trailing = realm.root.X.add();
    const firstValue = {
      first: equal,
      second: equal,
      proof: proveEquality(equal, equal),
      trailing,
    };
    const secondValue = {
      ...firstValue,
      proof: proveEquality(equal, equal),
    };
    const firstProof = realm.root.Accepted(firstValue).add();
    const secondProof = realm.root.Accepted(secondValue).add();
    realm.commit();

    assert.equal(valueEqual(firstProof, secondProof), true);
  },
);

test("proof-record-function-argument retains its erased equality", () => {
  const realm = beginRealm(ProofRecordFunctionArgumentRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const trailing = realm.root.X.add();
  const value = {
    first,
    second,
    proof: proveEquality(first, first),
    trailing,
  };
  realm.root.Accepted(value).add();

  assert.throws(() => realm.commit(), /\.Accepted\.foreignKey/);
});
