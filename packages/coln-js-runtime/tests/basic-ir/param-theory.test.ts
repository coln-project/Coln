// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ParamTheoryRealm from "../../../coln-compiler/test/golden/basic-ir/param-theory.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-theory", () => {
  const realm = beginRealm(ParamTheoryRealm);
  const point = realm.root.X.add();
  realm.root.P.point.set(point);
  const view = realm.commit();

  assert.equal(view.X.has(point), true);
  assert.equal(valueEqual(view.P.point.get(), point), true);
});
