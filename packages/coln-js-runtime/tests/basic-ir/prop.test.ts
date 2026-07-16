// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as PropRealm from "../../../coln-compiler/test/golden/basic-ir/prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop", () => {
  const realm = beginRealm(PropRealm);
  const value = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(value), true);
});
