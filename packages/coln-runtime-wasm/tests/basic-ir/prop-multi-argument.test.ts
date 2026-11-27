// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropMultiArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/prop-multi-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop-multi-argument", () => {
  const realm = beginRealm(PropMultiArgumentRealm);
  const value = realm.root.X.add();
  const firstProof = realm.root.P.add();
  const secondProof = realm.root.Q.add();
  const relationProof = realm.root.R(value)(firstProof)(secondProof).add();
  const view = realm.commit();

  assert.equal(view.R(value)(firstProof)(secondProof).has(relationProof), true);
});

test("prop-multi-argument keeps different Set parameters distinct", () => {
  const realm = beginRealm(PropMultiArgumentRealm);
  const firstValue = realm.root.X.add();
  const secondValue = realm.root.X.add();
  const firstProof = realm.root.P.add();
  const secondProof = realm.root.Q.add();
  const firstRelationProof = realm.root.R(firstValue)(firstProof)(secondProof).add();
  const secondRelationProof = realm.root.R(secondValue)(firstProof)(secondProof).add();
  realm.commit();

  assert.equal(valueEqual(firstRelationProof, secondRelationProof), false);
});
