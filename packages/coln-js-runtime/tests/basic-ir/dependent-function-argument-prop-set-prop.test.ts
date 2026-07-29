// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as DependentFunctionArgumentPropSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/dependent-function-argument-prop-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("dependent-function-argument-prop-set-prop", () => {
  const realm = beginRealm(DependentFunctionArgumentPropSetPropRealm);
  const a = realm.root.A.add();
  const b = realm.root.B(a).add();
  const c = realm.root.C(a)(b).add();
  realm.root.f(a)(b).set(c);
  const view = realm.commit();

  assert.equal(valueEqual(view.f(a)(b).get(), c), true);
});

test("dependent-function-argument-prop-set-prop is vacuous when its proof domain is empty", () => {
  const realm = beginRealm(DependentFunctionArgumentPropSetPropRealm);

  realm.commit();
});

test("dependent-function-argument-prop-set-prop requires an output when its codomain is uninhabited", () => {
  const realm = beginRealm(DependentFunctionArgumentPropSetPropRealm);
  const a = realm.root.A.add();
  realm.root.B(a).add();

  assert.throws(() => realm.commit(), /rule TRealm\.f/);
});

test("dependent-function-argument-prop-set-prop infers its output from an inhabited codomain", { expectFailure: true }, () => {
  const realm = beginRealm(DependentFunctionArgumentPropSetPropRealm);
  const a = realm.root.A.add();
  const b = realm.root.B(a).add();
  const c = realm.root.C(a)(b).add();
  const view = realm.commit();

  assert.equal(valueEqual(view.f(a)(b).get(), c), true);
});
