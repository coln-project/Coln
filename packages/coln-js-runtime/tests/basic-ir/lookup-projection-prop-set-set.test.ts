// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionPropSetSetRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-prop-set-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-projection-prop-set-set", () => {
  const realm = beginRealm(LookupProjectionPropSetSetRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.A.has(source), true);
  assert.equal(view.B.has(target), true);
  assert.equal(view.E(target).has(edge), true);
  assert.equal(valueEqual(view.next(source).get(), target), true);
  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});

test("lookup-projection-prop-set-set rejects an edge at a different target", () => {
  const realm = beginRealm(LookupProjectionPropSetSetRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);

  assert.throws(() => realm.commit(), /\.nextedge \.foreignKey/);
});
