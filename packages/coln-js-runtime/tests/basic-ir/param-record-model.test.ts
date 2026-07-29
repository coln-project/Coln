// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordModelRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-model.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-model", { expectFailure: true }, () => {
  const realm = beginRealm(ParamRecordModelRealm);
  const modelValue = realm.root.model.X.add();
  const box = {
    modelValue,
    value: { tag: "string", value: "payload" },
  } as const;
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
