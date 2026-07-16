// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as ProjectionRealm from "../../../coln-compiler/test/golden/basic-ir/projection.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("projection", () => {
  const realm = beginRealm(ProjectionRealm);
  const payload = {
    name: { tag: "string", value: "example" },
    rank: { tag: "int", value: 1 },
  } as const;
  const value = realm.root.E(payload.rank).add();
  realm.root.r(payload).set(value);
  const view = realm.commit();

  assert.equal(valueEqual(view.r(payload).get(), value), true);
});
