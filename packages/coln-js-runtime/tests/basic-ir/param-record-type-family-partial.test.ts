// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordTypeFamilyPartialRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-type-family-partial.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-type-family-partial", () => {
  const realm = beginRealm(ParamRecordTypeFamilyPartialRealm);
  const x = realm.root.X.add();
  const y = realm.root.Y.add();
  const entry = realm.root.R(x)(y).add();
  const slice = { entry };
  const value = realm.root.slices(x)(y)(slice).add();
  const view = realm.commit();

  assert.equal(view.slices(x)(y)(slice).has(value), true);
});
