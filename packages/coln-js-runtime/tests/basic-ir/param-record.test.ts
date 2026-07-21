// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordRealm from "../../../coln-compiler/test/golden/basic-ir/param-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("param-record", expectedFailure, () => {
  const realm = beginRealm(ParamRecordRealm);
  const value = realm.root.X.add();
  const box = { value };
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.X.has(value), true);
  assert.equal(view.boxed(box).has(boxed), true);
});
