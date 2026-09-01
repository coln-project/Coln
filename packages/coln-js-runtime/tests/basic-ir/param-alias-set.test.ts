// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamAliasSetRealm from "../../../coln-compiler/test/golden/basic-ir/param-alias-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-alias-set", { expectFailure: true }, () => {
  const realm = beginRealm(ParamAliasSetRealm);
  const box = { value: realm.root.X.add() };
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
