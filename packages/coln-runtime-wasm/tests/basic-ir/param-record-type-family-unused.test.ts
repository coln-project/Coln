// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordTypeFamilyUnusedRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-type-family-unused.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-type-family-unused", { expectFailure: true }, () => {
  const realm = beginRealm(ParamRecordTypeFamilyUnusedRealm);
  const box = {
    value: { tag: "int", value: 1 },
  } as const;
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
