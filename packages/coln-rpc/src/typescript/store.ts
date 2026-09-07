// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {WhereClause, WireRowId, Value, Path, CommitHash} from "./types.js"

export type WireValue = Value<WireRowId>

export type WireTuple = WireValue[]

export interface Store {
  commit(): CommitHash
  abort(): null

  all(query: WhereClause, select: [number]): [WireTuple]
  one(query: WhereClause, select: [number]): WireTuple
  exists(query: WhereClause): boolean
  
  add(table_name: Path, values: WireTuple): WireRowId
}
