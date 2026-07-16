// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProjectionSetSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-projection-set-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-projection-set-set-prop", () => {
  const realm = beginRealm(LookupProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.next(source).get(), target), true);
  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});
