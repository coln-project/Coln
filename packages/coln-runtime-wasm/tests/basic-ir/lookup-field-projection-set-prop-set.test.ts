// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupFieldProjectionSetPropSetRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-field-projection-set-prop-set.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-field-projection-set-prop-set", () => {
  const realm = beginRealm(LookupFieldProjectionSetPropSetRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.x.set(source);
  realm.root.next(source).set(target);
  realm.root.edge.set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.edge.get(), edge), true);
});
