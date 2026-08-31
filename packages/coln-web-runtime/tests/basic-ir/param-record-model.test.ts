// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordModelRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-model.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "model-parameterized record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("param-record-model", expectedFailure, () => {
  const realm = beginRealm(ParamRecordModelRealm);
  const modelValue = realm.root.model.X.add();
  const box = {
    modelValue,
    value: {name: {tag: "string", value: "payload"}},
  } as const;
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.model.X.has(modelValue), true);
  assert.equal(view.boxed(box).has(boxed), true);
});
