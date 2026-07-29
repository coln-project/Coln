// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropRecordRealm from "../../../coln-compiler/test/golden/basic-ir/prop-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop-record", { expectFailure: true }, () => {
  const realm = beginRealm(PropRecordRealm);
  const left = realm.root.P.add();
  const right = realm.root.Q.add();
  const pair = realm.root.make(left)(right);
  pair.left.set(left);
  pair.right.set(right);
  const view = realm.commit();
  const made = view.make(left)(right);

  assert.equal(valueEqual(made.left.get(), left), true);
  assert.equal(valueEqual(made.right.get(), right), true);
  assert.equal(
    valueEqual(view.projectLeft({ left, right }).get(), left),
    true,
  );
});
