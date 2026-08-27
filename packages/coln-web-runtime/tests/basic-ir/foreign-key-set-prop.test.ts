// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ForeignKeySetPropRealm from "../../../coln-compiler/test/golden/basic-ir/foreign-key-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedCanonicalizationFailure = {
  expectFailure: {
    label: "multiple proofs with the same parameter tuple are not canonicalized",
    match: /false !== true/,
  },
};

const expectedPendingCanonicalizationFailure = {
  expectFailure: {
    label: "pending proofs with the same parameter tuple are not canonicalized",
    match: /false !== true/,
  },
};

const expectedCardinalityFailure = {
  expectFailure: {
    label: "a proposition can contain multiple proof rows for one parameter tuple",
    match: /false !== true/,
  },
};

test("foreign-key-set-prop", () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  const edge = realm.root.E(vertex).add();
  const view = realm.commit();

  assert.equal(view.V.has(vertex), true);
  assert.equal(view.E(vertex).has(edge), true);
});

test("foreign-key-set-prop rejects a parameter from the wrong table", () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  const edge = realm.root.E(vertex).add();
  realm.root.E(edge).add();

  assert.throws(() => realm.commit(), /\.E\.foreignKey/);
});

test("foreign-key-set-prop canonicalizes proofs with the same parameter tuple", expectedCanonicalizationFailure, () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  const first = realm.root.E(vertex).add();
  const second = realm.root.E(vertex).add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});

test("foreign-key-set-prop canonicalizes pending proofs with the same parameter tuple", expectedPendingCanonicalizationFailure, () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  const first = realm.root.E(vertex).add();
  const second = realm.root.E(vertex).add();

  assert.equal(valueEqual(first, second), true);
});

test("foreign-key-set-prop keeps canonical proof handles valid", () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  const first = realm.root.E(vertex).add();
  const second = realm.root.E(vertex).add();
  const view = realm.commit();

  assert.equal(view.E(vertex).has(first), true);
  assert.equal(view.E(vertex).has(second), true);
});

test("foreign-key-set-prop keeps proofs for different parameter tuples distinct", () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const firstVertex = realm.root.V.add();
  const secondVertex = realm.root.V.add();
  const first = realm.root.E(firstVertex).add();
  const second = realm.root.E(secondVertex).add();
  realm.commit();

  assert.equal(valueEqual(first, second), false);
});

test("foreign-key-set-prop has at most one proof per parameter tuple", expectedCardinalityFailure, () => {
  const realm = beginRealm(ForeignKeySetPropRealm);
  const vertex = realm.root.V.add();
  realm.root.E(vertex).add();
  realm.root.E(vertex).add();
  const view = realm.commit();
  const proofs = view.E(vertex).values();

  assert.equal(proofs.next().done, false);
  assert.equal(proofs.next().done, true);
});
