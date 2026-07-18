// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as BuiltinRealm from "../../../coln-compiler/test/golden/basic-ir/builtin.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("builtin", () => {
  const realm = beginRealm(BuiltinRealm);
  const vertex = realm.root.V.add();
  const count = { tag: "int", value: 42 } as const;
  const label = { tag: "string", value: "example" } as const;
  realm.root.count(vertex).set(count);
  realm.root.label(vertex).set(label);
  const view = realm.commit();

  assert.equal(valueEqual(view.count(vertex).get(), count), true);
  assert.equal(valueEqual(view.label(vertex).get(), label), true);
});
