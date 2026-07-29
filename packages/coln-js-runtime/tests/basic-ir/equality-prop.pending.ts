// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import test from "node:test";

import * as EqualityPropRealm from "../../../coln-compiler/test/golden/basic-ir/equality-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("equality-prop compares erased proof operands", () => {
  const realm = beginRealm(EqualityPropRealm);
  const proof = realm.root.P.add();
  realm.root.x.set(proof);
  realm.root.y.set(proof);
  const view = realm.commit();

  view.eq.get();
});
