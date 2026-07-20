// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupLiteralPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-literal-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const intIndex = { tag: "int", value: 19 } as const;
const stringIndex = { tag: "string", value: "zombocom" } as const;
const expectedLiteralFailure = {
  expectFailure: {
    label: "literal terms use a different compiler and runtime JSON encoding",
    match: /unknown variant `int`, expected `lit` or `var`/,
  },
};

test("lookup-literal-prop", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact(intIndex).add();
  const stringProof = realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);
  const view = realm.commit();

  assert.equal(view.IntFact(intIndex).has(intProof), true);
  assert.equal(view.StringFact(stringIndex).has(stringProof), true);
  assert.equal(valueEqual(view.intFact.get(), intProof), true);
  assert.equal(valueEqual(view.stringFact.get(), stringProof), true);
});

test("lookup-literal-prop rejects a proof at a different integer", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact({ tag: "int", value: 20 }).add();
  const stringProof = realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);

  assert.throws(() => realm.commit(), /\.intFact\.foreignKey/);
});

test("lookup-literal-prop rejects a proof at a different string", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact(intIndex).add();
  const stringProof = realm.root.StringFact({
    tag: "string",
    value: "not-zombocom",
  }).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);

  assert.throws(() => realm.commit(), /\.stringFact\.foreignKey/);
});

test("lookup-literal-prop canonicalizes proofs at the same index", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const firstIntProof = realm.root.IntFact(intIndex).add();
  const secondIntProof = realm.root.IntFact(intIndex).add();
  const firstStringProof = realm.root.StringFact(stringIndex).add();
  const secondStringProof = realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(firstIntProof);
  realm.root.stringFact.set(firstStringProof);
  const view = realm.commit();

  assert.equal(valueEqual(firstIntProof, secondIntProof), true);
  assert.equal(valueEqual(firstStringProof, secondStringProof), true);
  assert.equal(view.IntFact(intIndex).has(firstIntProof), true);
  assert.equal(view.StringFact(stringIndex).has(firstStringProof), true);
});

test("lookup-literal-prop keeps proofs at different indexes distinct", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const firstIntProof = realm.root.IntFact(intIndex).add();
  const secondIntProof = realm.root.IntFact({ tag: "int", value: 20 }).add();
  const firstStringProof = realm.root.StringFact(stringIndex).add();
  const secondStringProof = realm.root.StringFact({
    tag: "string",
    value: "not-zombocom",
  }).add();
  realm.root.intFact.set(firstIntProof);
  realm.root.stringFact.set(firstStringProof);
  realm.commit();

  assert.equal(valueEqual(firstIntProof, secondIntProof), false);
  assert.equal(valueEqual(firstStringProof, secondStringProof), false);
});

test("lookup-literal-prop has at most one proof at each index", expectedLiteralFailure, () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact(intIndex).add();
  realm.root.IntFact(intIndex).add();
  const stringProof = realm.root.StringFact(stringIndex).add();
  realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);
  const view = realm.commit();

  const intProofs = view.IntFact(intIndex).values();
  assert.equal(intProofs.next().done, false);
  assert.equal(intProofs.next().done, true);

  const stringProofs = view.StringFact(stringIndex).values();
  assert.equal(stringProofs.next().done, false);
  assert.equal(stringProofs.next().done, true);
});
