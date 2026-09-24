// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as native from "../../native/index.js"
import type * as types from "@coln-project/interface"

type RealmDef = {
  ir: string,
  coln_source: string,
  realm_name: string
}

export class NodeStore implements types.Store {
  underlying: native.AutoStoreWrapper
  
  constructor(def: RealmDef) {
    this.underlying = native.storeFromIr(def.ir, def.coln_source, def.realm_name)
  }

  startTransaction(): void {
    this.underlying.startTransaction()
  }
  endTransaction(): types.CommitHash {
    return JSON.parse(this.underlying.endTransaction())
  }
  abortTransaction(): void {
    this.underlying.abortTransaction();
  }
  
  all_proj(where: types.WhereClause, select: number[]): types.WireTuple[] {
    return JSON.parse(this.underlying.allProj(JSON.stringify(where), JSON.stringify(select)))
  }
  
  all_row_id(where: types.WhereClause): types.WireRowId[] {
    return JSON.parse(this.underlying.allRowId(JSON.stringify(where)))
  }
  
  one_proj(where: types.WhereClause, select: number[]): types.WireTuple {
    return JSON.parse(this.underlying.oneProj(JSON.stringify(where), JSON.stringify(select)))
  }
  
  exists(where: types.WhereClause): boolean {
    return this.underlying.exists(JSON.stringify(where))
  }
  
  add(table_name: types.Path, values: types.TxnWireTuple): types.TxnWireRowId {
    return JSON.parse(this.underlying.add(table_name, JSON.stringify(values)))
  }
}
