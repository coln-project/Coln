// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropRecordRealm from "../../../coln-compiler/test/golden/basic-ir/prop-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFieldFailure = {
  expectFailure: {
    label: "proposition record fields are not exposed structurally",
    match: /Cannot read properties of undefined \(reading 'set'\)/,
  },
};

const expectedShapeFailure = {
  expectFailure: {
    label: "proposition records are exposed as table cells",
    match: /Expected values to be strictly deep-equal/,
  },
};

test("prop-record", expectedFieldFailure, () => {
  const realm = beginRealm(PropRecordRealm);
  const left = realm.root.P.add();
  const right = realm.root.Q.add();
  const pair = realm.root.make(left)(right);
  pair.left.set(left);
  pair.right.set(right);
  const view = realm.commit();

  assert.equal(view.P.has(left), true);
  assert.equal(view.Q.has(right), true);
  assert.equal(
    valueEqual(view.projectLeft({ left, right }).get(), left),
    true,
  );
});

test(
  "prop-record exposes its erased fields structurally",
  expectedShapeFailure,
  () => {
    const realm = beginRealm(PropRecordRealm);
    const left = realm.root.P.add();
    const right = realm.root.Q.add();

    assert.deepEqual(Object.keys(realm.root.make(left)(right)), [
      "left",
      "right",
    ]);
  },
);
