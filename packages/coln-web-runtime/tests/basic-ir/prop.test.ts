// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropRealm from "../../../coln-compiler/test/golden/basic-ir/prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedCanonicalizationFailure = {
  expectFailure: {
    label: "multiple proofs of a proposition are not canonicalized",
    match: /false !== true/,
  },
};

const expectedPendingCanonicalizationFailure = {
  expectFailure: {
    label: "pending proofs of a proposition are not canonicalized",
    match: /false !== true/,
  },
};

const expectedCardinalityFailure = {
  expectFailure: {
    label: "a proposition can contain multiple proof rows",
    match: /false !== true/,
  },
};

test("prop may be empty", () => {
  const realm = beginRealm(PropRealm);
  const view = realm.commit();

  assert.equal(view.V.values().next().done, true);
});

test("prop", () => {
  const realm = beginRealm(PropRealm);
  const value = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(value), true);
});

test("prop canonicalizes proofs", expectedCanonicalizationFailure, () => {
  const realm = beginRealm(PropRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});

test("prop canonicalizes pending proofs", expectedPendingCanonicalizationFailure, () => {
  const realm = beginRealm(PropRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();

  assert.equal(valueEqual(first, second), true);
});

test("prop keeps canonical proof handles valid", () => {
  const realm = beginRealm(PropRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(first), true);
  assert.equal(view.V.has(second), true);
});

test("prop has at most one proof", expectedCardinalityFailure, () => {
  const realm = beginRealm(PropRealm);
  realm.root.V.add();
  realm.root.V.add();
  const view = realm.commit();
  const proofs = view.V.values();

  assert.equal(proofs.next().done, false);
  assert.equal(proofs.next().done, true);
});
