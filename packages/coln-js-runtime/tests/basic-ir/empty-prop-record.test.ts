// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EmptyPropRecordRealm from "../../../coln-compiler/test/golden/basic-ir/empty-prop-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "empty proposition records are exposed as table cells",
    match: /Expected values to be strictly deep-equal/,
  },
};

test("empty-prop-record", expectedFailure, () => {
  const realm = beginRealm(EmptyPropRecordRealm);

  assert.deepEqual(realm.root.truth, {});
  assert.deepEqual(realm.commit().truth, {});
});
