// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as FamilyArgumentSetPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/family-argument-set-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("family-argument-set-prop-prop", () => {
  const realm = beginRealm(FamilyArgumentSetPropPropRealm);
  const a = realm.root.A.add();
  const b = realm.root.B(a).add();
  const r = realm.root.R(a)(b).add();
  const view = realm.commit();

  assert.equal(view.R(a)(b).has(r), true);
});
