import {WhereClause, TxnWireRowId, WireRowId, Value, Path, CommitHash} from "./types.js"

export type WireValue = Value<WireRowId>
export type TxnWireValue = Value<TxnWireRowId>

export type WireTuple = WireValue[]
export type TxnWireTuple = TxnWireValue[]

export interface Store {
  commit(): CommitHash
  abort(): null

  all_proj(query: WhereClause, select: [number]): [WireTuple]
  all_row_id(query: WhereClause): [WireRowId]
  one_proj(query: WhereClause, select: [number]): WireTuple
  // don't need one_row_id
  exists(query: WhereClause): boolean
  
  add(table_name: Path, values: TxnWireTuple): WireRowId
}
