// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as PropRecordNestedDependentRealm from "../../../coln-compiler/test/golden/basic-ir/prop-record-nested-dependent.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop-record-nested-dependent", { expectFailure: true }, () => {
  const realm = beginRealm(PropRecordNestedDependentRealm);
  const value = realm.root.X.add();
  const nested = realm.root.P(value).add();
  const direct = realm.root.Q(value).add();
  const proof = realm.root.make(value);
  proof.nested.proof.set(nested);
  proof.direct.set(direct);
  const view = realm.commit();
  const result = view.make(value);

  assert.equal(valueEqual(result.nested.proof.get(), nested), true);
  assert.equal(valueEqual(result.direct.get(), direct), true);
});

test("prop-record-nested-dependent retains its nested result obligation", () => {
  const realm = beginRealm(PropRecordNestedDependentRealm);
  const value = realm.root.X.add();
  realm.root.Q(value).add();

  assert.throws(() => realm.commit(), /rule TRealm\.make/);
});

test("prop-record-nested-dependent retains its direct result obligation", () => {
  const realm = beginRealm(PropRecordNestedDependentRealm);
  const value = realm.root.X.add();
  realm.root.P(value).add();

  assert.throws(() => realm.commit(), /rule TRealm\.make/);
});
