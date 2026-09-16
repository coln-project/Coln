// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordDependentFunctionRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-dependent-function.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-dependent-function", { expectFailure: true }, () => {
  const realm = beginRealm(ParamRecordDependentFunctionRealm);
  const key = realm.root.Key.add();
  realm.root.key.set(key);
  const image = realm.root.E(key).add();
  realm.root.f(key).set(image);
  const box = { image };
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
