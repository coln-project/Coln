// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EmptyTheoryRealm from "../../../coln-compiler/test/golden/basic-ir/empty-theory.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("empty-theory", () => {
  const realm = beginRealm(EmptyTheoryRealm);

  assert.deepEqual(realm.root, {});
  assert.deepEqual(realm.commit(), {});
});
