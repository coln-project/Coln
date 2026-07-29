// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/function-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("function-set-prop", () => {
  const realm = beginRealm(FunctionSetPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-set-prop requires an output when its codomain is uninhabited", () => {
  const realm = beginRealm(FunctionSetPropRealm);
  realm.root.X.add();

  assert.throws(() => realm.commit(), /rule TRealm\.next/);
});

test("function-set-prop infers its output from an inhabited codomain", () => {
  const realm = beginRealm(FunctionSetPropRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function-set-prop returns equal proofs for different inputs", () => {
  const realm = beginRealm(FunctionSetPropRealm);
  const firstInput = realm.root.X.add();
  const secondInput = realm.root.X.add();
  const firstOutput = realm.root.Y.add();
  const secondOutput = realm.root.Y.add();
  realm.root.next(firstInput).set(firstOutput);
  realm.root.next(secondInput).set(secondOutput);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(firstInput).get(), view.next(secondInput).get()), true);
});
