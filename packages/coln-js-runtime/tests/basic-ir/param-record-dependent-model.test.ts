// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordDependentModelRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-dependent-model.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("param-record-dependent-model", () => {
  const realm = beginRealm(ParamRecordDependentModelRealm);
  const rank = { tag: "int", value: 1 } as const;
  const key = { rank };
  const value = { tag: "string", value: "payload" } as const;
  realm.root.pointed.point.rank.set(rank);
  const box = { key, value };
  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
