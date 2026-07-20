// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupCompositionPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-composition-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedInputFailure = {
  expectFailure: {
    label: "composition treats equal source proofs as distinct",
    match: /\.first\.total/,
  },
};

const expectedIntermediateFailure = {
  expectFailure: {
    label: "composition treats equal intermediate proofs as distinct",
    match: /\.second\.total/,
  },
};

const expectedEqualTargetFailure = {
  expectFailure: {
    label: "equal composed target proofs identify different propositions",
    match: /\.edge\.foreignKey/,
  },
};

const expectedOutputFailure = {
  expectFailure: {
    label: "proof-valued composition can return a distinct proof",
    match: /false !== true/,
  },
};

const expectedImplicitCompositionFailure = {
  expectFailure: {
    label: "composition through inhabited propositions is not synthesized",
    match: /\.first\.total/,
  },
};

test("lookup-composition-prop", () => {
  const realm = beginRealm(LookupCompositionPropRealm);
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

test("lookup-composition-prop ignores proof identity in its source", expectedInputFailure, () => {
  const realm = beginRealm(LookupCompositionPropRealm);
  const firstSource = realm.root.A.add();
  const secondSource = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  realm.root.first(firstSource).set(intermediate);
  realm.root.second(intermediate).set(target);
  realm.root.edge(firstSource).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.first(secondSource).get(), intermediate), true);
  assert.equal(valueEqual(view.edge(secondSource).get(), edge), true);
});

test("lookup-composition-prop ignores proof identity at its intermediate lookup", expectedIntermediateFailure, () => {
  const realm = beginRealm(LookupCompositionPropRealm);
  const source = realm.root.A.add();
  const firstIntermediate = realm.root.B.add();
  const secondIntermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  realm.root.first(source).set(firstIntermediate);
  realm.root.second(secondIntermediate).set(target);
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.second(firstIntermediate).get(), target), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});

test("lookup-composition-prop accepts an edge at an equal composed target", expectedEqualTargetFailure, () => {
  const realm = beginRealm(LookupCompositionPropRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const firstTarget = realm.root.C.add();
  const secondTarget = realm.root.C.add();
  const edge = realm.root.E(secondTarget).add();
  realm.root.first(source).set(intermediate);
  realm.root.second(intermediate).set(firstTarget);
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(view.E(firstTarget).has(edge), true);
});

test("lookup-composition-prop returns a canonical edge proof", expectedOutputFailure, () => {
  const realm = beginRealm(LookupCompositionPropRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const firstEdge = realm.root.E(target).add();
  const secondEdge = realm.root.E(target).add();
  realm.root.first(source).set(intermediate);
  realm.root.second(intermediate).set(target);
  realm.root.edge(source).set(firstEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.edge(source).get(), secondEdge), true);
});

test("lookup-composition-prop infers an inhabited composition", expectedImplicitCompositionFailure, () => {
  const realm = beginRealm(LookupCompositionPropRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  const view = realm.commit();

  assert.equal(valueEqual(view.first(source).get(), intermediate), true);
  assert.equal(valueEqual(view.second(intermediate).get(), target), true);
  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});
