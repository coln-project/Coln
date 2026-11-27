// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import assert from "node:assert/strict";
import test from "node:test";

import { valueEqual } from "@coln-project/runtime";
import * as LookupArgumentProjectionSetSetPropRealm from "../../../coln-compiler/test/golden/basic-ir/lookup-argument-projection-set-set-prop.ts.output/TRealm.ts";
import { beginRealm } from "./helpers.ts";

test("lookup-argument-projection-set-set-prop", () => {
  const realm = beginRealm(LookupArgumentProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B(source).add();
  const edge = realm.root.E(source)(target).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});

test("lookup-argument-projection-set-set-prop rejects an edge at a different target", () => {
  const realm = beginRealm(LookupArgumentProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B(source).add();
  const otherTarget = realm.root.B(source).add();
  const edge = realm.root.E(source)(otherTarget).add();
  realm.root.next(source).set(target);
  realm.root.nextedge(source).set(edge);

  assert.throws(() => realm.commit(), /rule TRealm\.nextedge/);
});

test("lookup-argument-projection-set-set-prop infers its output from an inhabited proposition", { expectFailure: true }, () => {
  const realm = beginRealm(LookupArgumentProjectionSetSetPropRealm);
  const source = realm.root.A.add();
  const target = realm.root.B(source).add();
  const edge = realm.root.E(source)(target).add();
  realm.root.next(source).set(target);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(source).get(), edge), true);
});

test("lookup-argument-projection-set-set-prop keeps proofs for different lookup results distinct", () => {
  const realm = beginRealm(LookupArgumentProjectionSetSetPropRealm);
  const firstSource = realm.root.A.add();
  const secondSource = realm.root.A.add();
  const firstTarget = realm.root.B(firstSource).add();
  const secondTarget = realm.root.B(secondSource).add();
  const firstEdge = realm.root.E(firstSource)(firstTarget).add();
  const secondEdge = realm.root.E(secondSource)(secondTarget).add();
  realm.root.next(firstSource).set(firstTarget);
  realm.root.next(secondSource).set(secondTarget);
  realm.root.nextedge(firstSource).set(firstEdge);
  realm.root.nextedge(secondSource).set(secondEdge);
  const view = realm.commit();

  assert.equal(valueEqual(view.nextedge(firstSource).get(), view.nextedge(secondSource).get()), false);
});
