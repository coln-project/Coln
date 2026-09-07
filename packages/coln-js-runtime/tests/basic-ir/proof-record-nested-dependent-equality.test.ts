// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ProofRecordNestedDependentEqualityRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-nested-dependent-equality.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-nested-dependent-equality", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordNestedDependentEqualityRealm);
  const expected = realm.root.X.add();
  const result = realm.root.result(expected);
  result.actual.set(expected);
  const view = realm.commit();

  view.result(expected).evidence.proof.get();
});

test("proof-record-nested-dependent-equality retains its equality", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordNestedDependentEqualityRealm);
  const expected = realm.root.X.add();
  const actual = realm.root.X.add();
  const result = realm.root.result(expected);
  result.actual.set(actual);

  assert.throws(() => realm.commit(), /\.result\.foreignKey/);
});
