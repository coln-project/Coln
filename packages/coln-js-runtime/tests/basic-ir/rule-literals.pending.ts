// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as RuleLiteralsRealm from "../../../coln-compiler/test/golden/basic-ir/rule-literals.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const count = { tag: "int", value: 19 } as const;
const label = { tag: "string", value: "zombocom" } as const;

test("rule-literals", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set(count);
  realm.root.label(value).set(label);
  realm.root.countIs19(value).add();
  realm.root.labelIsZombocom(value).add();
  const view = realm.commit();

  assert.equal(valueEqual(view.count(value).get(), count), true);
  assert.equal(valueEqual(view.label(value).get(), label), true);
});

test("rule-literals rejects a different integer", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set({ tag: "int", value: 20 });
  realm.root.label(value).set(label);
  realm.root.countIs19(value).add();
  realm.root.labelIsZombocom(value).add();

  assert.throws(() => realm.commit(), /\.countIs19\.foreignKey/);
});

test("rule-literals rejects a different string", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set(count);
  realm.root.label(value).set({ tag: "string", value: "not-zombocom" });
  realm.root.countIs19(value).add();
  realm.root.labelIsZombocom(value).add();

  assert.throws(() => realm.commit(), /\.labelIsZombocom\.foreignKey/);
});
