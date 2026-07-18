// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ForeignKeyPropSetRealm from "../../../coln-compiler/test/golden/basic-ir/foreign-key-prop-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("foreign-key-prop-set", () => {
  const realm = beginRealm(ForeignKeyPropSetRealm);
  const vertex = realm.root.V.add();
  const edge = realm.root.E(vertex).add();
  const view = realm.commit();

  assert.equal(view.V.has(vertex), true);
  assert.equal(view.E(vertex).has(edge), true);
});

test("foreign-key-prop-set rejects a parameter from the wrong table", () => {
  const realm = beginRealm(ForeignKeyPropSetRealm);
  const vertex = realm.root.V.add();
  const edge = realm.root.E(vertex).add();
  realm.root.E(edge).add();

  assert.throws(() => realm.commit(), /\.E\.foreignKey/);
});
