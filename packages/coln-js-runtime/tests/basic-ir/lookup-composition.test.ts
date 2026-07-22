// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupCompositionRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-composition.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-composition", () => {
  const realm = beginRealm(LookupCompositionRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  realm.root.first(source).set(intermediate);
  realm.root.second(intermediate).set(target);
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.A.has(source), true);
  assert.equal(view.B.has(intermediate), true);
  assert.equal(view.C.has(target), true);
  assert.equal(view.E(target).has(edge), true);
  assert.equal(valueEqual(view.first(source).get(), intermediate), true);
  assert.equal(valueEqual(view.second(intermediate).get(), target), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});

test("lookup-composition rejects an edge at a different composed target", () => {
  const realm = beginRealm(LookupCompositionRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const otherTarget = realm.root.C.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.first(source).set(intermediate);
  realm.root.second(intermediate).set(target);
  realm.root.edge(source).set(edge);

  assert.throws(() => realm.commit(), /\.edge\.foreignKey/);
});
