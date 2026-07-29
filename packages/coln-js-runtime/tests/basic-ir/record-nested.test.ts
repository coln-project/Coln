// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as RecordNestedRealm from "../../../coln-compiler/test/golden/basic-ir/record-nested.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("record-nested", () => {
  const realm = beginRealm(RecordNestedRealm);
  const name = { tag: "string", value: "example" } as const;
  const rank = { tag: "int", value: 1 } as const;
  realm.root.payload.inner.rank.set(rank);
  realm.root.payload.name.set(name);
  const view = realm.commit();

  assert.equal(valueEqual(view.payload.name.get(), name), true);
  assert.equal(valueEqual(view.payload.inner.rank.get(), rank), true);
});
