// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProjectionRealm from "../../../coln-compiler/test/golden/basic-ir/projection.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("projection", () => {
  const realm = beginRealm(ProjectionRealm);
  const a = realm.root.X.add();
  const b = realm.root.X.add();
  const valueForA = realm.root.E(a).add();
  const valueForB = realm.root.E(b).add();
  realm.root.r({ first: a, second: a }).set(valueForA);
  realm.root.r({ first: a, second: b }).set(valueForB);
  realm.root.r({ first: b, second: a }).set(valueForA);
  realm.root.r({ first: b, second: b }).set(valueForB);
  const view = realm.commit();

  assert.equal(
    valueEqual(view.r({ first: a, second: b }).get(), valueForB),
    true,
  );
});

test("projection rejects a value at a different projected value", () => {
  const realm = beginRealm(ProjectionRealm);
  const a = realm.root.X.add();
  const b = realm.root.X.add();
  const valueForA = realm.root.E(a).add();
  const valueForB = realm.root.E(b).add();
  realm.root.r({ first: a, second: a }).set(valueForA);
  realm.root.r({ first: a, second: b }).set(valueForA);
  realm.root.r({ first: b, second: a }).set(valueForA);
  realm.root.r({ first: b, second: b }).set(valueForB);

  assert.throws(() => realm.commit(), /\.r\.foreignKey/);
});
