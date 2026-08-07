// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropRealm from "../../../coln-compiler/test/golden/basic-ir/prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

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

test("prop canonicalizes proofs", { expectFailure: true }, () => {
  const realm = beginRealm(PropRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});

test("prop keeps canonical proof handles valid", () => {
  const realm = beginRealm(PropRealm);
  realm.root.V.add();
  const canonical = realm.root.V.add();
  const view = realm.commit();

  assert.equal(view.V.has(canonical), true);
});

test("prop has at most one proof", { expectFailure: true }, () => {
  const realm = beginRealm(PropRealm);
  realm.root.V.add();
  realm.root.V.add();
  const view = realm.commit();
  const proofs = view.V.values();

  assert.equal(proofs.next().done, false);
  assert.equal(proofs.next().done, true);
});
