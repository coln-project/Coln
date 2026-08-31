// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ParamTheoryModelRealm from "../../../coln-compiler/test/golden/basic-ir/param-theory-model.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-theory-model", () => {
  const realm = beginRealm(ParamTheoryModelRealm);
  const point = realm.root.model.X.add();
  realm.root.pointed.point.set(point);
  const view = realm.commit();

  assert.equal(view.model.X.has(point), true);
  assert.equal(valueEqual(view.pointed.point.get(), point), true);
});
