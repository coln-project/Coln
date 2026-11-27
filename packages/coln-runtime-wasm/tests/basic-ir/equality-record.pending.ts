// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EqualityRecordRealm from "../../../coln-compiler/test/golden/basic-ir/equality-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality-record compares every stored field", () => {
  const realm = beginRealm(EqualityRecordRealm);
  const left = realm.root.X.add();
  const right = realm.root.X.add();
  realm.root.first.left.set(left);
  realm.root.first.right.set(right);
  realm.root.second.left.set(left);
  realm.root.second.right.set(right);
  const view = realm.commit();

  view.same.get();
});

test("equality-record rejects a difference in one field", () => {
  const realm = beginRealm(EqualityRecordRealm);
  const first = realm.root.X.add();
  const second = realm.root.X.add();
  realm.root.first.left.set(first);
  realm.root.first.right.set(first);
  realm.root.second.left.set(first);
  realm.root.second.right.set(second);

  assert.throws(() => realm.commit(), /rule TRealm\.same/);
});
