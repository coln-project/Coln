// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EmptyRecordRealm from "../../../coln-compiler/test/golden/basic-ir/empty-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "empty records are emitted as table cells rather than sets",
    match: /realm\.root\.unit\.add is not a function/,
  },
};

test("empty-record", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const value = realm.root.unit.add();
  const view = realm.commit();

  assert.equal(view.unit.has(value), true);
});
