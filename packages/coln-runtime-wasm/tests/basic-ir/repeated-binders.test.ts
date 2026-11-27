// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as RepeatedBindersRealm from "../../../coln-compiler/test/golden/basic-ir/repeated-binders.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("repeated-binders", () => {
  const realm = beginRealm(RepeatedBindersRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const pair = realm.root.Pair(first)(second).add();
  const view = realm.commit();

  assert.equal(view.Pair(first)(second).has(pair), true);
  assert.equal(view.Pair(second)(first).has(pair), false);
});
