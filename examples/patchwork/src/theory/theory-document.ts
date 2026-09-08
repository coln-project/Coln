// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { PatchworkHandle, PatchworkMeta } from "../patchwork.ts"

export type RealmSchema = {
  entities: unknown[]
  rules: unknown[]
}

export type StoreRecord = {
  /** The `coln:` url of the store itself. */
  url: string
  createdAt: number
  sourceHeads: string[]
  realmIndex: number
  realmName: string
  ir: RealmSchema
  /**
   * The Patchwork pointer document opened to browse this store, when one has
   * been made. Absent on records written by Coln Lab, which had no Patchwork
   * documents to point with — the Store Editor makes one on demand.
   */
  docUrl?: string
}

export type TheoryDocument = {
  "@patchwork"?: PatchworkMeta
  version: 1
  title: string
  source: string
  stores: StoreRecord[]
}

export type TheoryHandle = PatchworkHandle<TheoryDocument>

export function isTheoryDocument(value: unknown): value is TheoryDocument {
  if (typeof value !== "object" || value === null) return false
  const document = value as Partial<TheoryDocument>
  return document.version === 1
    && typeof document.source === "string"
    && Array.isArray(document.stores)
    && document.stores.every(isStoreRecord)
}

export function isRealmSchema(value: unknown): value is RealmSchema {
  if (typeof value !== "object" || value === null) return false
  const schema = value as Partial<RealmSchema>
  return Array.isArray(schema.entities) && Array.isArray(schema.rules)
}

function isStoreRecord(value: unknown): value is StoreRecord {
  if (typeof value !== "object" || value === null) return false
  const store = value as Partial<StoreRecord>
  return typeof store.url === "string"
    && typeof store.createdAt === "number"
    && Number.isFinite(store.createdAt)
    && Array.isArray(store.sourceHeads)
    && store.sourceHeads.every(head => typeof head === "string")
    && Number.isInteger(store.realmIndex)
    && typeof store.realmName === "string"
    && isRealmSchema(store.ir)
}
