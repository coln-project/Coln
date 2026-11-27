// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as native from "#wasm-bodge/bindings"
import type * as types from "@coln-project/interface"

type RealmDef = {
  ir: string,
  coln_source: string,
  realm_name: string
}

export class WasmStore implements types.Store {
  underlying: native.StoreWrapper
  
  constructor(def: RealmDef) {
    this.underlying = native.store_from_ir(def.ir, def.coln_source, def.realm_name)
  }

  startTransaction(): void {
    this.underlying.start_transaction()
  }
  endTransaction(): types.CommitHash {
    return JSON.parse(this.underlying.end_transaction())
  }
  abortTransaction(): void {
    this.underlying.abort_transaction();
  }
  
  all_proj(where: types.WhereClause, select: number[]): types.WireTuple[] {
    return JSON.parse(this.underlying.all_proj(JSON.stringify(where), JSON.stringify(select)))
  }
  
  all_row_id(where: types.WhereClause): types.WireRowId[] {
    return JSON.parse(this.underlying.all_row_id(JSON.stringify(where)))
  }
  
  one_proj(where: types.WhereClause, select: number[]): types.WireTuple {
    return JSON.parse(this.underlying.one_proj(JSON.stringify(where), JSON.stringify(select)))
  }
  
  exists(where: types.WhereClause): boolean {
    return this.underlying.exists(JSON.stringify(where))
  }
  
  add(table_name: types.Path, values: types.TxnWireTuple): types.TxnWireRowId {
    return JSON.parse(this.underlying.add(table_name, JSON.stringify(values)))
  }
}
