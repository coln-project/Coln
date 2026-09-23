// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as TheoryAliasRealm from "../../../coln-compiler/test/golden/basic-ir/theory-alias.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("theory-alias", () => {
  const realm = beginRealm(TheoryAliasRealm);
  const value = realm.root.X.add();
  const view = realm.commit();

  assert.equal(view.X.has(value), true);
});
