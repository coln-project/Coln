// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type {
  AutomergeUrl,
  CrdtDocHandle,
  DocumentType,
  Repo,
} from "@automerge/automerge-repo";
import type { RealmBindings } from "@coln-project/runtime";
import {
  colnDocType,
  type ColnChange,
  type ColnDocument,
  type ColnSchema,
  type ColnState,
} from "./colnDocType.js";

type FoundColnHandle<Bindings extends RealmBindings | undefined> =
  Bindings extends RealmBindings ? ColnHandle<Bindings> : ColnHandle;

export type ColnHandle<Bindings extends RealmBindings | undefined = undefined> =
  CrdtDocHandle<
    DocumentType<ColnState, ColnDocument<Bindings>, ColnChange<Bindings>, ColnSchema>
  >;

export function applyBindings<Bindings extends RealmBindings>(
  handle: ColnHandle | ColnHandle<RealmBindings>,
  bindings: Bindings,
): ColnHandle<Bindings> {
  const state = handle.fullDoc();
  const existingBindings = state.bindings;

  if (existingBindings) {
    if (existingBindings !== bindings) {
      throw new TypeError("Coln handle already uses different realm bindings");
    }
    return handle as unknown as ColnHandle<Bindings>;
  }

  const actualSchema = canonicalJson(JSON.parse(handle.doc().jsonIR()));
  const bindingsSchema = canonicalJson(bindings.schema);
  if (actualSchema !== bindingsSchema) {
    throw new TypeError("Realm bindings schema does not match Coln document schema");
  }

  state.bindings = bindings;

  return handle as unknown as ColnHandle<Bindings>;
}

export function create<Bindings extends RealmBindings>(
  repo: Repo,
  bindings: Bindings,
): ColnHandle<Bindings> {
  const handle = repo.create(bindings.schema, colnDocType);

  return applyBindings(handle, bindings);
}

export async function find<Bindings extends RealmBindings | undefined = undefined>(
  repo: Repo,
  url: AutomergeUrl,
  // a rest tuple (rather than an optional parameter) keeps `undefined` in the
  // inferred Bindings, so maybe-undefined arguments produce a union return type
  ...rest: [bindings: Bindings] | []
): Promise<FoundColnHandle<Bindings>> {
  const [bindings] = rest;
  const handle = await repo.find(url, colnDocType);

  return (bindings === undefined ? handle : applyBindings(handle, bindings)) as FoundColnHandle<Bindings>;
}

function canonicalJson(value: unknown): string | undefined {
  return JSON.stringify(value, (_key, current: unknown) => {
    if (current === null || Array.isArray(current) || typeof current !== "object") return current;
    return Object.fromEntries(
      Object.entries(current).sort(([left], [right]) => left.localeCompare(right)),
    );
  });
}
