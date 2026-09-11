// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { DocHandle } from "@automerge/automerge-repo"

export type RealmSchema = {
  entities: unknown[]
  rules: unknown[]
}

export type StoreRecord = {
  url: string
  createdAt: number
  sourceHeads: string[]
  realmIndex: number
  realmName: string
  ir: RealmSchema
}

export type TheoryDocument = {
  version: 1
  source: string
  stores: StoreRecord[]
}

export type TheoryDocumentHandle = DocHandle<TheoryDocument>

export function newTheoryDocument(): TheoryDocument {
  return { version: 1, source: "", stores: [] }
}

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
