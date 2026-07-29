// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamAliasRealm from "../../../coln-compiler/test/golden/basic-ir/param-alias.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-alias", () => {
  const realm = beginRealm(ParamAliasRealm);
  const box = {
    key: { tag: "int", value: 1 },
    modelValue: realm.root.model.X.add(),
    value: { tag: "string", value: "payload" },
  } as const;
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
