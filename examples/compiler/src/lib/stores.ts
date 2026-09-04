// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Repo } from "@automerge/automerge-repo"
import { colnDocType } from "@coln-project/repo"
import type { CompiledRealm } from "./compiled-realms.ts"
import type { TheoryHandle } from "./theory-handle.svelte.ts"
import type { StoreRecord } from "./theory-document.ts"

export async function createStore(
  repo: Repo,
  theory: TheoryHandle,
  realm: CompiledRealm,
): Promise<{ record: StoreRecord; theoryFlushError?: unknown }> {
  const createdAt = Date.now()
  const sourceHeads = theory.heads
  const store = repo.create(realm.schema, colnDocType)
  const record: StoreRecord = {
    url: store.url,
    createdAt,
    sourceHeads,
    realmIndex: realm.index,
    realmName: realm.name,
    ir: realm.schema,
  }

  await repo.flush([store.documentId])
  theory.appendStore(record)
  try {
    await repo.flush([theory.documentId])
  } catch (theoryFlushError) {
    return { record, theoryFlushError }
  }

  return { record }
}
