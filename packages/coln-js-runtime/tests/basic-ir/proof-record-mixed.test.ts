// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProofRecordMixedRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-mixed.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record fields are not exposed through the generated table cell",
    match: /Cannot read properties of undefined \(reading 'set'\)/,
  },
};

test("proof-record-mixed", expectedFailure, () => {
  const realm = beginRealm(ProofRecordMixedRealm);
  const equal = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.value.first.set(equal);
  realm.root.value.second.set(equal);
  realm.root.value.proof.set(equal);
  realm.root.value.trailing.set(trailing);
  const view = realm.commit();

  assert.equal(valueEqual(view.value.first.get(), equal), true);
  assert.equal(valueEqual(view.value.second.get(), equal), true);
  assert.equal(valueEqual(view.value.trailing.get(), trailing), true);
});

test("proof-record-mixed retains its erased equality", expectedFailure, () => {
  const realm = beginRealm(ProofRecordMixedRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.value.first.set(first);
  realm.root.value.second.set(second);
  realm.root.value.proof.set(first);
  realm.root.value.trailing.set(trailing);

  assert.throws(() => realm.commit(), /\.value\.foreignKey/);
});
