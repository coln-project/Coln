// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/function-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("function-prop-prop", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-prop-prop is vacuous when its domain is empty", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const view = realm.commit();

  assert.equal(view.Y.values().next().done, true);
});

test("function-prop-prop requires an output when its codomain is uninhabited", () => {
  const realm = beginRealm(FunctionPropPropRealm);
  realm.root.X.add();

  assert.throws(() => realm.commit(), /rule TRealm\.next/);
});

test("function-prop-prop infers its output from an inhabited codomain", { expectFailure: true }, () => {
  const realm = beginRealm(FunctionPropPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});
