// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProofRecordNestedDependentPropRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-nested-dependent-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-nested-dependent-prop", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordNestedDependentPropRealm);
  const value = { tag: "int", value: 1 } as const;
  const proof = realm.root.P(value).add();
  realm.root.package.value.set(value);
  const view = realm.commit();

  assert.equal(valueEqual(view.package.evidence.proof.get(), proof), true);
});

test("proof-record-nested-dependent-prop retains its proof obligation", { expectFailure: true }, () => {
  const realm = beginRealm(ProofRecordNestedDependentPropRealm);
  const value = { tag: "int", value: 1 } as const;
  realm.root.package.value.set(value);

  assert.throws(() => realm.commit(), /\.package\.foreignKey/);
});
