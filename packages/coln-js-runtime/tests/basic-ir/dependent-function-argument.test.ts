// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as DependentFunctionArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/dependent-function-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("dependent-function-argument", () => {
  const realm = beginRealm(DependentFunctionArgumentRealm);
  const a = realm.root.A.add();
  const b = realm.root.B(a).add();
  const c = realm.root.C(a)(b).add();
  realm.root.f(a)(b).set(c);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(a)(b).get(), c), true);
});
