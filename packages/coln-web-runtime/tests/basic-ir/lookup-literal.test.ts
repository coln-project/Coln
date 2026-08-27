// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupLiteralRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-literal.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const intIndex = { tag: "int", value: 19 } as const;
const stringIndex = { tag: "string", value: "zombocom" } as const;

test("lookup-literal", () => {
  const realm = beginRealm(LookupLiteralRealm);
  const intEdge = realm.root.IntEdge(intIndex).add();
  const stringEdge = realm.root.StringEdge(stringIndex).add();
  realm.root.intEdge.set(intEdge);
  realm.root.stringEdge.set(stringEdge);
  const view = realm.commit();

  assert.equal(view.IntEdge(intIndex).has(intEdge), true);
  assert.equal(view.StringEdge(stringIndex).has(stringEdge), true);
  assert.equal(valueEqual(view.intEdge.get(), intEdge), true);
  assert.equal(valueEqual(view.stringEdge.get(), stringEdge), true);
});

test("lookup-literal rejects an edge at a different integer", () => {
  const realm = beginRealm(LookupLiteralRealm);
  const intEdge = realm.root.IntEdge({ tag: "int", value: 20 }).add();
  const stringEdge = realm.root.StringEdge(stringIndex).add();
  realm.root.intEdge.set(intEdge);
  realm.root.stringEdge.set(stringEdge);

  assert.throws(() => realm.commit(), /\.intEdge\.foreignKey/);
});

test("lookup-literal rejects an edge at a different string", () => {
  const realm = beginRealm(LookupLiteralRealm);
  const intEdge = realm.root.IntEdge(intIndex).add();
  const stringEdge = realm.root.StringEdge({
    tag: "string",
    value: "not-zombocom",
  }).add();
  realm.root.intEdge.set(intEdge);
  realm.root.stringEdge.set(stringEdge);

  assert.throws(() => realm.commit(), /\.stringEdge\.foreignKey/);
});
