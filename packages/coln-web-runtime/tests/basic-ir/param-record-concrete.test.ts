// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import * as ParamRecordConcreteRealm from "../../../coln-compiler/test/golden/basic-ir/param-record-concrete.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

const expectedFailure = {
  expectFailure: {
    label: "concrete nested record values are not implemented by the runtime",
    match: /missing field `tag`/,
  },
};

test("param-record-concrete", expectedFailure, () => {
  const realm = beginRealm(ParamRecordConcreteRealm);
  const box = {
    value: {
      name: { tag: "string", value: "payload" },
    },
  } as const;

  const boxed = realm.root.boxed(box).add();
  const view = realm.commit();

  assert.equal(view.boxed(box).has(boxed), true);
});
