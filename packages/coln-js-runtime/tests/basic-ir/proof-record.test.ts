// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ProofRecordRealm from "../../../coln-compiler/test/golden/basic-ir/proof-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("proof-record", () => {
  const realm = beginRealm(ProofRecordRealm);
  const value = realm.root.X.add();
  const proof = realm.root.witness(value).add();
  const view = realm.commit();

  assert.equal(view.X.has(value), true);
  assert.equal(view.witness(value).has(proof), true);
});
