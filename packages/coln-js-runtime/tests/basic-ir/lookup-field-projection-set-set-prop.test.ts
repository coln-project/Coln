// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupFieldProjectionSetSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-field-projection-set-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-field-projection-set-set-prop", () => {
  const realm = beginRealm(LookupFieldProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const edge = realm.root.E(target).add();
  realm.root.x.set(source);
  realm.root.next(source).set(target);
  realm.root.edge.set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.edge.get(), edge), true);
});

test("lookup-field-projection-set-set-prop rejects an edge at a different target", () => {
  const realm = beginRealm(LookupFieldProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B.add();
  const otherTarget = realm.root.B.add();
  const edge = realm.root.E(otherTarget).add();
  realm.root.x.set(source);
  realm.root.next(source).set(target);
  realm.root.edge.set(edge);

  assert.throws(() => realm.commit(), /rule TRealm\.edge/);
});
