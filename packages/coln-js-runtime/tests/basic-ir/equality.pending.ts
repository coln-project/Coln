// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as EqualityRealm from "../../../coln-compiler/test/golden/basic-ir/equality.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality is inhabited without add for equal values", () => {
  const realm = beginRealm(EqualityRealm);
  const value = realm.root.V.add();
  realm.root.x.set(value);
  realm.root.y.set(value);
  const view = realm.commit();
  const proofs = view.eq.values();

  assert.equal(proofs.next().done, false);
  assert.equal(proofs.next().done, true);
});

test("equality is not inhabited for unequal values", () => {
  const realm = beginRealm(EqualityRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.root.x.set(first);
  realm.root.y.set(second);

  assert.throws(() => realm.commit(), /\.eq\.total/);
});

test("equality canonicalizes proofs", () => {
  const realm = beginRealm(EqualityRealm);
  const value = realm.root.V.add();
  realm.root.x.set(value);
  realm.root.y.set(value);
  const first = realm.root.eq.add();
  const second = realm.root.eq.add();
  const view = realm.commit();
  const proofs = view.eq.values();

  assert.equal(valueEqual(first, second), true);
  assert.equal(proofs.next().done, false);
  assert.equal(proofs.next().done, true);
});

test("equality rejects a proof of unequal values", () => {
  const realm = beginRealm(EqualityRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.root.x.set(first);
  realm.root.y.set(second);
  realm.root.eq.add();

  assert.throws(() => realm.commit(), /\.eq\.foreignKey/);
});
