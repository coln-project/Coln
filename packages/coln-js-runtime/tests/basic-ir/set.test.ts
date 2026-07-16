// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as SetRealm from "../../../coln-compiler/test/basic-ir/set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("set", () => {
  const realm = beginRealm(SetRealm);
  const value = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(value), true);
  assert.equal(view.V.values().next().done, false);
});
