// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as RecordRealm from "../../../coln-compiler/test/basic-ir/record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("record", () => {
  const realm = beginRealm(RecordRealm);
  const point = realm.root.point.add();

  assert.ok(realm.root.payload(point));
});
