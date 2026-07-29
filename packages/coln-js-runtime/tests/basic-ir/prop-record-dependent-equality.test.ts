// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import test from "node:test";

import * as PropRecordDependentEqualityRealm from "../../../coln-compiler/test/golden/basic-ir/prop-record-dependent-equality.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("prop-record-dependent-equality retains evidence used by a later equality field", () => {
  const realm = beginRealm(PropRecordDependentEqualityRealm);
  realm.root.P.add();
  const view = realm.commit();

  view.witness.same.get();
});
