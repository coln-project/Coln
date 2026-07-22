// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionSetPropSetRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-set-prop-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedProofIrrelevanceFailure = {
  expectFailure: {
    label: "sets indexed by equal lookup proofs are treated as distinct",
    match: /false !== true/,
  },
};

const expectedEqualTargetFailure = {
  expectFailure: {
    label: "equal lookup proofs identify different edge sets",
    match: /\.nextedge\.foreignKey/,
  },
};

test("lookup-projection-set-prop-set", () => {
  const realm = beginRealm(LookupProjectionSetPropSetRealm);
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

test("lookup-projection-set-prop-set accepts an edge at an equal proof target", expectedEqualTargetFailure, () => {
  const realm = beginRealm(LookupProjectionSetPropSetRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.E(target).has(edge), true);
  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});

test("lookup-projection-set-prop-set ignores proof identity in lookup results", expectedProofIrrelevanceFailure, () => {
  const realm = beginRealm(LookupProjectionSetPropSetRealm);
  const firstTarget = realm.root.B.add();
  const secondTarget = realm.root.B.add();
  const edge = realm.root.E(firstTarget).add();
  const view = realm.commit();

  assert.equal(view.E(secondTarget).has(edge), true);
});
