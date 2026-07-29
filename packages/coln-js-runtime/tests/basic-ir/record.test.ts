// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as RecordRealm from "../../../coln-compiler/test/golden/basic-ir/record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("record", () => {
  const realm = beginRealm(RecordRealm);
  const point = realm.root.point.add();
  const name = { tag: "string", value: "example" } as const;
  const rank = { tag: "int", value: 1 } as const;
  const payload = realm.root.payload(point);
  payload.rank.set(rank);
  payload.name.set(name);
  const view = realm.commit();

  assert.equal(valueEqual(view.payload(point).name.get(), name), true);
  assert.equal(valueEqual(view.payload(point).rank.get(), rank), true);
});
