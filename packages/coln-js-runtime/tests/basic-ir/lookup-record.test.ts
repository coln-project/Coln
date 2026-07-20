// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupRecordRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("lookup-record", expectedFailure, () => {
  const realm = beginRealm(LookupRecordRealm);
  const fixed = {
    name: { tag: "string", value: "fixed" },
    rank: { tag: "int", value: 1 },
  } as const;
  const selected = realm.root.E(fixed).add();
  realm.root.selected.set(selected);
  const view = realm.commit();

  assert.equal(view.E(fixed).has(selected), true);
  assert.equal(valueEqual(view.selected.get(), selected), true);
});
