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

test("lookup-literal-prop", () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact(intIndex).add();
  const stringProof = realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);
  const view = realm.commit();

  assert.equal(valueEqual(view.intFact.get(), intProof), true);
  assert.equal(valueEqual(view.stringFact.get(), stringProof), true);
});

test("lookup-literal-prop rejects a proof at a different integer", () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact({ tag: "int", value: 20 }).add();
  const stringProof = realm.root.StringFact(stringIndex).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);

  assert.throws(() => realm.commit(), /rule TRealm\.intFact/);
});

test("lookup-literal-prop rejects a proof at a different string", () => {
  const realm = beginRealm(LookupLiteralPropRealm);
  const intProof = realm.root.IntFact(intIndex).add();
  const stringProof = realm.root.StringFact({
    tag: "string",
    value: "not-zombocom",
  }).add();
  realm.root.intFact.set(intProof);
  realm.root.stringFact.set(stringProof);

  assert.throws(() => realm.commit(), /rule TRealm\.stringFact/);
});
