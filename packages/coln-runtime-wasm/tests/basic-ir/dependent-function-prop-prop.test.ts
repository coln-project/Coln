// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as DependentFunctionPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/dependent-function-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("dependent-function-prop-prop", () => {
  const realm = beginRealm(DependentFunctionPropPropRealm);
  const a = realm.root.A.add();
  const b = realm.root.B(a).add();
  realm.root.f(a).set(b);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(a).get(), b), true);
});
