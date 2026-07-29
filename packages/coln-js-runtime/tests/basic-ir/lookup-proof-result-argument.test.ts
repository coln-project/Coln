// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupProofResultArgumentRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-proof-result-argument.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-proof-result-argument", () => {
  const realm = beginRealm(LookupProofResultArgumentRealm);
  const x = realm.root.X.add();
  const p = realm.root.P(x).add();
  const r = realm.root.R(x)(p).add();
  realm.root.witness(x).set(p);
  realm.root.use(x).set(r);
  const view = realm.commit();

  assert.equal(valueEqual(view.use(x).get(), r), true);
});
