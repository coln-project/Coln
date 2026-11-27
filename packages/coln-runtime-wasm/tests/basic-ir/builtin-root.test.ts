// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as BuiltinRootRealm from "../../../coln-compiler/test/golden/basic-ir/builtin-root.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("builtin-root", () => {
  const realm = beginRealm(BuiltinRootRealm);
  const count = { tag: "int", value: 42 } as const;
  const label = { tag: "string", value: "example" } as const;
  realm.root.count.set(count);
  realm.root.label.set(label);
  const view = realm.commit();

  assert.equal(valueEqual(view.count.get(), count), true);
  assert.equal(valueEqual(view.label.get(), label), true);
});
