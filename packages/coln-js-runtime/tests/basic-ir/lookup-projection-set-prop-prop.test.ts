// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionSetPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-set-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedEqualTargetFailure = {
  expectFailure: {
    label: "equal lookup proofs identify different propositions",
    match: /\.nextedge\.foreignKey/,
  },
};

const expectedOutputFailure = {
  expectFailure: {
    label: "proof-valued lookups can return distinct proofs of equal propositions",
    match: /false !== true/,
  },
};

test("lookup-projection-set-prop-prop", () => {
  const realm = beginRealm(LookupProjectionSetPropPropRealm);
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

test("lookup-projection-set-prop-prop accepts an edge at an equal proof target", expectedEqualTargetFailure, () => {
  const realm = beginRealm(LookupProjectionSetPropPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.E(target).has(edge), true);
});

test("lookup-projection-set-prop-prop returns equal target proofs", expectedOutputFailure, () => {
  const realm = beginRealm(LookupProjectionSetPropPropRealm);
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

  assert.equal(valueEqual(view.next(firstSource).get(), view.next(secondSource).get()), true);
});

test("lookup-projection-set-prop-prop returns equal edge proofs", expectedOutputFailure, () => {
  const realm = beginRealm(LookupProjectionSetPropPropRealm);
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

  assert.equal(valueEqual(view.nextedge(firstSource).get(), view.nextedge(secondSource).get()), true);
});
