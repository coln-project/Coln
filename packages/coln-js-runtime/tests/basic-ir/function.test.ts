// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as FunctionRealm from "../../../coln-compiler/test/golden/basic-ir/function.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("function", () => {
  const realm = beginRealm(FunctionRealm);
  const input = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(output);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(input).get(), output), true);
});

test("function requires an output for every input", () => {
  const realm = beginRealm(FunctionRealm);
  realm.root.X.add();

  assert.throws(() => realm.commit(), /\.next \.total/);
});

test("function rejects an output from the wrong table", () => {
  const realm = beginRealm(FunctionRealm);
  const input = realm.root.X.add();
  const wrongOutput = realm.root.X.add();
  const output = realm.root.Y.add();
  realm.root.next(input).set(wrongOutput);
  realm.root.next(wrongOutput).set(output);

  assert.throws(() => realm.commit(), /\.next \.foreignKey/);
});
