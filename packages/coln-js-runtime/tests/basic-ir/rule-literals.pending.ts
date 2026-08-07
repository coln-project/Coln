// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as RuleLiteralsRealm from "../../../coln-compiler/test/golden/basic-ir/rule-literals.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const count = { tag: "int", value: 19 } as const;
const label = { tag: "string", value: "zombocom" } as const;

test("rule-literals", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set(count);
  realm.root.label(value).set(label);
  const view = realm.commit();

  view.countIs19(value).get();
  view.labelIsZombocom(value).get();
});

test("rule-literals rejects a different integer", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set({ tag: "int", value: 20 });
  realm.root.label(value).set(label);

  assert.throws(() => realm.commit(), /rule TRealm\.countIs19/);
});

test("rule-literals rejects a different string", () => {
  const realm = beginRealm(RuleLiteralsRealm);
  const value = realm.root.X.add();
  realm.root.count(value).set(count);
  realm.root.label(value).set({ tag: "string", value: "not-zombocom" });

  assert.throws(() => realm.commit(), /rule TRealm\.labelIsZombocom/);
});
