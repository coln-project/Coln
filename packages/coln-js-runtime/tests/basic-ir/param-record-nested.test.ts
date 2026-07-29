// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordNestedRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-nested.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-nested", () => {
  const realm = beginRealm(ParamRecordNestedRealm);
  const inner = realm.root.X.add();
  const nested = { inner: { value: inner } };
  const value = realm.root.nested(nested).add();
  const view = realm.commit();

  assert.equal(view.nested(nested).has(value), true);
});
