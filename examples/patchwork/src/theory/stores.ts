// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { colnDocType } from "@coln-project/repo"
import { colnRepo, trackDocument } from "../coln/repo.ts"
import { createPointerDoc } from "../coln/pointer.ts"
import { STORE_TYPE } from "../store/datatype.ts"
import type { CompiledRealm } from "./compiled-realms.ts"
import type { StoreRecord, TheoryHandle } from "./theory-document.ts"

/**
 * Create a Coln store for one compiled realm, then record it on the theory.
 *
 * Two documents in two repos come out of this: the store itself in Coln's repo
 * (flushed to Coln's relay before it is linked, so a collaborator following
 * the link finds something there), and a Patchwork pointer document so the
 * store can be opened as a Patchwork document at all.
 */
export async function createStore(
  theory: TheoryHandle,
  realm: CompiledRealm,
): Promise<StoreRecord> {
  const repo = colnRepo()
  const createdAt = Date.now()
  const sourceHeads = [...theory.heads()]
  const store = repo.create(realm.schema, colnDocType)

  trackDocument(store.documentId)
  await repo.flush([store.documentId])

  const docUrl = await createPointerDoc({
    type: STORE_TYPE,
    title: `${realm.name} store`,
    storeUrl: store.url,
    realmName: realm.name,
    theoryUrl: theory.url,
  })

  const record: StoreRecord = {
    url: store.url,
    createdAt,
    sourceHeads,
    realmIndex: realm.index,
    realmName: realm.name,
    ir: realm.schema,
    docUrl,
  }
  theory.change(document => {
    document.stores.push(record)
  })

  return record
}

