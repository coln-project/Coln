// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EqualityRecordNestedRealm from "../../../coln-compiler/test/golden/basic-ir/equality-record-nested.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality-record-nested compares empty, stored, and proof fields", () => {
  const realm = beginRealm(EqualityRecordNestedRealm);
  realm.root.P.add();
  const inner = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.first.inner.value.set(inner);
  realm.root.first.trailing.set(trailing);
  realm.root.second.inner.value.set(inner);
  realm.root.second.trailing.set(trailing);
  const view = realm.commit();

  view.same.get();
});

test("equality-record-nested rejects a nested stored difference", () => {
  const realm = beginRealm(EqualityRecordNestedRealm);
  realm.root.P.add();
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  const trailing = realm.root.X.add();
  realm.root.first.inner.value.set(first);
  realm.root.first.trailing.set(trailing);
  realm.root.second.inner.value.set(second);
  realm.root.second.trailing.set(trailing);

  assert.throws(() => realm.commit(), /rule TRealm\.same/);
});
