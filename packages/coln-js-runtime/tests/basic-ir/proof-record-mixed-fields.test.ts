// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProofRecordMixedFieldsRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-mixed-fields.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-mixed-fields", () => {
  const realm = beginRealm(ProofRecordMixedFieldsRealm);
  const equal = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.value.first.set(equal);
  realm.root.value.second.set(equal);
  realm.root.value.trailing.set(trailing);
  const view = realm.commit();

  view.value.proof.get();
  assert.equal(valueEqual(view.value.trailing.get(), trailing), true);
});

test("proof-record-mixed-fields retains its erased equality", () => {
  const realm = beginRealm(ProofRecordMixedFieldsRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.value.first.set(first);
  realm.root.value.second.set(second);
  realm.root.value.trailing.set(trailing);

  assert.throws(() => realm.commit(), /\.value\.foreignKey/);
});
