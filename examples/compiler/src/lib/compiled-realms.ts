// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { isRealmSchema, type RealmSchema } from "./theory-document.ts"

export type CompiledRealm = {
  index: number
  name: string
  schema: RealmSchema
}

export function parseCompiledRealms(irJson: string): CompiledRealm[] {
  if (irJson === "") return []
  const value: unknown = JSON.parse(irJson)
  if (!Array.isArray(value)) throw new TypeError("Compiler IR must be an array of realms")

  return value.map((schema, index) => {
    if (!isRealmSchema(schema)) {
      throw new TypeError(`Compiler IR realm ${index + 1} is malformed`)
    }
    return { index, name: realmName(schema, index), schema }
  })
}

function realmName(schema: RealmSchema, index: number): string {
  for (const entry of [...schema.entities, ...schema.rules]) {
    if (typeof entry !== "object" || entry === null) continue
    const path = (entry as { path?: unknown }).path
    if (!Array.isArray(path) || !Array.isArray(path[0])) continue
    if (path[0].every(part => typeof part === "string")) return path[0].join(".")
  }
  return `Realm ${index + 1}`
}
