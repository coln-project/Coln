// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupCompositionPropPropPropPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-composition-prop-prop-prop-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-composition-prop-prop-prop-prop", () => {
  const realm = beginRealm(LookupCompositionPropPropPropPropRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  realm.root.first(source).set(intermediate);
  realm.root.second(intermediate).set(target);
  realm.root.edge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});

test("lookup-composition-prop-prop-prop-prop infers an inhabited composition", { expectFailure: true }, () => {
  const realm = beginRealm(LookupCompositionPropPropPropPropRealm);
  const source = realm.root.A.add();
  const intermediate = realm.root.B.add();
  const target = realm.root.C.add();
  const edge = realm.root.E(target).add();
  const view = realm.commit();

  assert.equal(valueEqual(view.edge(source).get(), edge), true);
});
