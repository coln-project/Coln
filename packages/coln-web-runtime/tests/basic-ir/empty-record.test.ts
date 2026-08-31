// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as EmptyRecordRealm from "../../../coln-compiler/test/golden/basic-ir/empty-record.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "empty records are emitted as table cells rather than sets",
    match: /realm\.root\.unit\.add is not a function/,
  },
};

const expectedImplicitInhabitantFailure = {
  expectFailure: {
    label: "the empty record is not inhabited automatically",
    match: /\.unit\.total/,
  },
};

test("empty-record is inhabited without add", expectedImplicitInhabitantFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const view = realm.commit();
  const values = view.unit.values();

  assert.equal(values.next().done, false);
  assert.equal(values.next().done, true);
});

test("empty-record", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const value = realm.root.unit.add();
  const view = realm.commit();

  assert.equal(view.unit.has(value), true);
});

test("empty-record canonicalizes values", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const first = realm.root.unit.add();
  const second = realm.root.unit.add();
  realm.commit();

  assert.equal(valueEqual(first, second), true);
});

test("empty-record canonicalizes pending values", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const first = realm.root.unit.add();
  const second = realm.root.unit.add();

  assert.equal(valueEqual(first, second), true);
});

test("empty-record keeps canonical value handles valid", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  const first = realm.root.unit.add();
  const second = realm.root.unit.add();
  const view = realm.commit();

  assert.equal(view.unit.has(first), true);
  assert.equal(view.unit.has(second), true);
});

test("empty-record has exactly one value", expectedFailure, () => {
  const realm = beginRealm(EmptyRecordRealm);
  realm.root.unit.add();
  realm.root.unit.add();
  const view = realm.commit();
  const values = view.unit.values();

  assert.equal(values.next().done, false);
  assert.equal(values.next().done, true);
});
