// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as EqualityPropRealm from "../../../coln-compiler/test/golden/basic-ir/equality-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality-prop is inhabited without add", () => {
  const realm = beginRealm(EqualityPropRealm);
  const firstProof = realm.root.P.add();
  const secondProof = realm.root.P.add();
  realm.root.x.set(firstProof);
  realm.root.y.set(secondProof);
  const view = realm.commit();
  const equalityProofs = view.eq.values();

  assert.equal(equalityProofs.next().done, false);
  assert.equal(equalityProofs.next().done, true);
});

test("equality-prop accepts equality between proof handles", () => {
  const realm = beginRealm(EqualityPropRealm);
  const firstProof = realm.root.P.add();
  const secondProof = realm.root.P.add();
  realm.root.x.set(firstProof);
  realm.root.y.set(secondProof);
  const equalityProof = realm.root.eq.add();
  const view = realm.commit();

  assert.equal(valueEqual(view.x.get(), view.y.get()), true);
  assert.equal(view.eq.has(equalityProof), true);
});

test("equality-prop canonicalizes equality proofs", () => {
  const realm = beginRealm(EqualityPropRealm);
  const proof = realm.root.P.add();
  realm.root.x.set(proof);
  realm.root.y.set(proof);
  const first = realm.root.eq.add();
  const second = realm.root.eq.add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});

test("equality-prop canonicalizes pending equality proofs", () => {
  const realm = beginRealm(EqualityPropRealm);
  const proof = realm.root.P.add();
  realm.root.x.set(proof);
  realm.root.y.set(proof);
  const first = realm.root.eq.add();
  const second = realm.root.eq.add();

  assert.equal(valueEqual(first, second), true);
});

test("equality-prop keeps canonical equality proof handles valid", () => {
  const realm = beginRealm(EqualityPropRealm);
  const proof = realm.root.P.add();
  realm.root.x.set(proof);
  realm.root.y.set(proof);
  const first = realm.root.eq.add();
  const second = realm.root.eq.add();
  const view = realm.commit();

  assert.equal(view.eq.has(first), true);
  assert.equal(view.eq.has(second), true);
});

test("equality-prop has exactly one equality proof", () => {
  const realm = beginRealm(EqualityPropRealm);
  const proof = realm.root.P.add();
  realm.root.x.set(proof);
  realm.root.y.set(proof);
  realm.root.eq.add();
  realm.root.eq.add();
  const view = realm.commit();
  const equalityProofs = view.eq.values();

  assert.equal(equalityProofs.next().done, false);
  assert.equal(equalityProofs.next().done, true);
});
