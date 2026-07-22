// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionSetSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-set-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedProofIrrelevanceFailure = {
  expectFailure: {
    label: "lookups can return distinct proofs of the same proposition",
    match: /false !== true/,
  },
};

const expectedImplicitOutputFailure = {
  expectFailure: {
    label: "a dependent function into an inhabited proposition is not synthesized",
    match: /\.nextedge\.total/,
  },
};

test("lookup-projection-set-set-prop", () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
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

test("lookup-projection-set-set-prop rejects an edge at a different target", () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);

  assert.throws(() => realm.commit(), /\.nextedge\.foreignKey/);
});

test("lookup-projection-set-set-prop infers its output from an inhabited proposition", expectedImplicitOutputFailure, () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.next(source).set(target);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});

test("lookup-projection-set-set-prop returns equal proofs for equal lookup results", expectedProofIrrelevanceFailure, () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
  const firstSource = realm.root.A.add();
  const secondSource = realm.root.A.add();
  const target = realm.root.B.add();
  const firstEdge = realm.root.E(target).add();
  const secondEdge = realm.root.E(target).add();
  realm.root.next(firstSource).set(target);
  realm.root.next(secondSource).set(target);
  realm.root.nextedge(firstSource).set(firstEdge);
  realm.root.nextedge(secondSource).set(secondEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(firstSource).get(), view.nextedge(secondSource).get()), true);
});

test("lookup-projection-set-set-prop keeps proofs for different lookup results distinct", () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
  const firstSource = realm.root.A.add();
  const secondSource = realm.root.A.add();
  const firstTarget = realm.root.B.add();
  const secondTarget = realm.root.B.add();
  const firstEdge = realm.root.E(firstTarget).add();
  const secondEdge = realm.root.E(secondTarget).add();
  realm.root.next(firstSource).set(firstTarget);
  realm.root.next(secondSource).set(secondTarget);
  realm.root.nextedge(firstSource).set(firstEdge);
  realm.root.nextedge(secondSource).set(secondEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(firstSource).get(), view.nextedge(secondSource).get()), false);
});
