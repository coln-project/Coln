// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionPropPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-prop-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedInputFailure = {
  expectFailure: {
    label: "lookups treat equal proof inputs as distinct",
    match: /\.next\.total/,
  },
};

const expectedEqualTargetFailure = {
  expectFailure: {
    label: "equal lookup proofs identify different propositions",
    match: /\.nextedge\.foreignKey/,
  },
};

const expectedOutputFailure = {
  expectFailure: {
    label: "proof-valued lookups can return distinct proofs",
    match: /false !== true/,
  },
};

test("lookup-projection-prop-prop-prop", () => {
  const realm = beginRealm(LookupProjectionPropPropPropRealm);
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

test("lookup-projection-prop-prop-prop ignores proof identity in its input", expectedInputFailure, () => {
  const realm = beginRealm(LookupProjectionPropPropPropRealm);
  const firstSource = realm.root.A.add();
  const secondSource = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.next(firstSource).set(target);
  realm.root.nextedge(firstSource).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(secondSource).get(), target), true);
  assert.equal(valueEqual(view.nextedge(secondSource).get(), edge), true);
});

test("lookup-projection-prop-prop-prop accepts an edge at an equal proof target", expectedEqualTargetFailure, () => {
  const realm = beginRealm(LookupProjectionPropPropPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.E(target).has(edge), true);
});

test("lookup-projection-prop-prop-prop returns a canonical target proof", expectedOutputFailure, () => {
  const realm = beginRealm(LookupProjectionPropPropPropRealm);
  const source = realm.root.A.add();
  const firstTarget = realm.root.B.add();
  const secondTarget = realm.root.B.add();
  const edge = realm.root.E(firstTarget).add();
  realm.root.next(source).set(firstTarget);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(source).get(), secondTarget), true);
});

test("lookup-projection-prop-prop-prop returns a canonical edge proof", expectedOutputFailure, () => {
  const realm = beginRealm(LookupProjectionPropPropPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const firstEdge = realm.root.E(target).add();
  const secondEdge = realm.root.E(target).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(firstEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(source).get(), secondEdge), true);
});
