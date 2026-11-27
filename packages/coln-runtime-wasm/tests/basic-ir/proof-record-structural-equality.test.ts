// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ProofRecordStructuralEqualityRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-structural-equality.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-structural-equality exposes its equality field", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordStructuralEqualityRealm);
  const left = realm.root.X.add();
  const right = realm.root.X.add();
  realm.root.comparison.first.left.set(left);
  realm.root.comparison.first.right.set(right);
  realm.root.comparison.second.left.set(left);
  realm.root.comparison.second.right.set(right);
  const view = realm.commit();

  view.comparison.same.get();
});

test("proof-record-structural-equality retains every leaf equality", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordStructuralEqualityRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  realm.root.comparison.first.left.set(first);
  realm.root.comparison.first.right.set(first);
  realm.root.comparison.second.left.set(first);
  realm.root.comparison.second.right.set(second);

  assert.throws(() => realm.commit(), /\.comparison\.foreignKey/);
});
