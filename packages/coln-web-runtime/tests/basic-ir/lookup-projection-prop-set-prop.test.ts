// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionPropSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-prop-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedInputFailure = {
  expectFailure: {
    label: "lookups treat equal proof inputs as distinct",
    match: /\.next\.total/,
  },
};

const expectedOutputFailure = {
  expectFailure: {
    label: "a proposition at one lookup target can contain distinct proofs",
    match: /false !== true/,
  },
};

test("lookup-projection-prop-set-prop", () => {
  const realm = beginRealm(LookupProjectionPropSetPropRealm);
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

test("lookup-projection-prop-set-prop rejects an edge at a different target", () => {
  const realm = beginRealm(LookupProjectionPropSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);

  assert.throws(() => realm.commit(), /\.nextedge\.foreignKey/);
});

test("lookup-projection-prop-set-prop ignores proof identity in its input", expectedInputFailure, () => {
  const realm = beginRealm(LookupProjectionPropSetPropRealm);
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

test("lookup-projection-prop-set-prop returns a canonical edge proof", expectedOutputFailure, () => {
  const realm = beginRealm(LookupProjectionPropSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const firstEdge = realm.root.E(target).add();
  const secondEdge = realm.root.E(target).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(firstEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(source).get(), secondEdge), true);
});
