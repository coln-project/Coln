// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as SetRealm from "../../../coln-compiler/test/golden/basic-ir/set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("set", () => {
  const realm = beginRealm(SetRealm);
  const value = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(value), true);
  assert.equal(value.tag, "row_id");
  if (value.tag !== "row_id") {
    assert.fail("set insertion did not return a row ID");
  }
  const values = view.V.values();
  const first = values.next();
  assert.equal(first.done, false);
  if (first.done) {
    assert.fail("committed set was empty");
  }
  assert.deepEqual(first.value, { rowId: value.value, values: [] });
  assert.equal(values.next().done, true);
});
