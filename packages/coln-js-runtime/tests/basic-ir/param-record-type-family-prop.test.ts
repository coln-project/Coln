// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordTypeFamilyPropRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-type-family-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-type-family-prop", () => {
  const realm = beginRealm(ParamRecordTypeFamilyPropRealm);
  const x = realm.root.X.add();
  const proof = realm.root.P(x).add();
  const evidence = { proof };
  const value = realm.root.evidence(x)(evidence).add();
  const view = realm.commit();

  assert.equal(view.evidence(x)(evidence).has(value), true);
});
