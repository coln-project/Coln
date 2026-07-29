// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropAliasRealm from "../../../coln-compiler/test/golden/basic-ir/prop-alias.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop-alias canonicalizes proofs", () => {
  const realm = beginRealm(PropAliasRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});
