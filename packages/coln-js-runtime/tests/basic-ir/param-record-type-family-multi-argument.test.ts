// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordTypeFamilyMultiArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-type-family-multi-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-type-family-multi-argument", { expectFailure: true }, () => {
  const realm = beginRealm(ParamRecordTypeFamilyMultiArgumentRealm);
  const x = realm.root.X.add();
  const y = realm.root.Y.add();
  const entry = realm.root.R(x)(y).add();
  const cell = { entry };
  const value = realm.root.cells(x)(y)(cell).add();
  const view = realm.commit();

  assert.equal(view.cells(x)(y)(cell).has(value), true);
});
