// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ParamTheoryNestedRealm from "../../../coln-compiler/test/golden/basic-ir/param-theory-nested.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-theory-nested", () => {
  const realm = beginRealm(ParamTheoryNestedRealm);
  const point = realm.root.X.add();
  realm.root.outer.inner.point.set(point);
  const view = realm.commit();

  assert.equal(view.X.has(point), true);
  assert.equal(valueEqual(view.outer.inner.point.get(), point), true);
});
