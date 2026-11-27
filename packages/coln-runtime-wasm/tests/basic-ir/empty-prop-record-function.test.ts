// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as EmptyPropRecordFunctionRealm from "../../../coln-compiler/test/golden/basic-ir/empty-prop-record-function.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("empty-prop-record-function", { expectFailure: true }, () => {
  const realm = beginRealm(EmptyPropRecordFunctionRealm);
  const value = realm.root.X.add();

  assert.deepEqual(realm.root.trivial(value), {});

  const view = realm.commit();

  assert.deepEqual(view.trivial(value), {});
});
