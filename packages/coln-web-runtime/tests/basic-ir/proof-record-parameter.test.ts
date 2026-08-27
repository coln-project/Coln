// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProofRecordParameterRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-parameter.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record arguments are passed as unflattened runtime values",
    match: /missing field `tag`/,
  },
};

test("proof-record-parameter", expectedFailure, () => {
  const realm = beginRealm(ProofRecordParameterRealm);
  const equal = realm.root.X.add();
  const value = {
    first: equal,
    second: equal,
    proof: equal,
    trailing: equal,
  };
  const accepted = realm.root.Accepted(value).add();
  realm.root.select(value).set(equal);
  const view = realm.commit();

  assert.equal(view.Accepted(value).has(accepted), true);
  assert.equal(valueEqual(view.select(value).get(), equal), true);
});

test(
  "proof-record-parameter ignores its nested proof handle",
  expectedFailure,
  () => {
    const realm = beginRealm(ProofRecordParameterRealm);
    const equal = realm.root.X.add();
    const trailing = realm.root.X.add();
    const firstValue = {
      first: equal,
      second: equal,
      proof: equal,
      trailing,
    };
    const secondValue = { ...firstValue, proof: trailing };
    const firstProof = realm.root.Accepted(firstValue).add();
    const secondProof = realm.root.Accepted(secondValue).add();
    for (const equalField of [equal, trailing]) {
      for (const trailingField of [equal, trailing]) {
        realm.root
          .select({
            first: equalField,
            second: equalField,
            proof: equal,
            trailing: trailingField,
          })
          .set(trailingField);
      }
    }
    realm.commit();

    assert.equal(valueEqual(firstProof, secondProof), true);
  },
);

test("proof-record-parameter retains its erased equality", expectedFailure, () => {
  const realm = beginRealm(ProofRecordParameterRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const trailing = realm.root.X.add();
  const value = { first, second, proof: first, trailing };
  realm.root.Accepted(value).add();

  assert.throws(() => realm.commit(), /\.Accepted\.foreignKey/);
});
