// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProofRecordPropFieldRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record-prop-field.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record-prop-field", () => {
  const realm = beginRealm(ProofRecordPropFieldRealm);
  const proof = realm.root.P.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.evidence.proof.get(), proof), true);
});
