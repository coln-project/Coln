// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordTypeFamilyLambdaRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-type-family-lambda.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-type-family-lambda", () => {
  const realm = beginRealm(ParamRecordTypeFamilyLambdaRealm);
  const x = { tag: "int", value: 1 } as const;
  const box = {
    value: { tag: "string", value: "payload" },
  } as const;
  const boxed = realm.root.boxed(x)(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(x)(box).has(boxed), true);
});
