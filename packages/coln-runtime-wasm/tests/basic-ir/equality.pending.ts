// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EqualityRealm from "../../../coln-compiler/test/golden/basic-ir/equality.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality produces a proof for equal values", () => {
  const realm = beginRealm(EqualityRealm);
  const value = realm.root.V.add();
  realm.root.x.set(value);
  realm.root.y.set(value);
  const view = realm.commit();

  view.eq.get();
});

test("equality rejects unequal values", () => {
  const realm = beginRealm(EqualityRealm);
  const first = realm.root.V.add();
  const second = realm.root.V.add();
  realm.root.x.set(first);
  realm.root.y.set(second);

  assert.throws(() => realm.commit(), /rule TRealm\.eq/);
});
