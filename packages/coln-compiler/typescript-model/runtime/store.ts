import { RowId } from "./row_id.js"
import { WireTuple, TxnWireTuple } from "./value.ts"
import { WhereClause, TxnWireRowId, WireRowId, Path, CommitHash } from "./types.js"

export interface Store {
  commit(): CommitHash
  abort(): void

  all_proj(query: WhereClause, select: number[]): [WireTuple]
  all_row_id(query: WhereClause): [WireRowId]
  one_proj(query: WhereClause, select: number[]): WireTuple
  // don't need one_row_id
  exists(query: WhereClause): boolean
  
  add(table_name: Path, values: TxnWireTuple): TxnWireRowId
}

export class ManagedStore {
  private pending: RowId<any>[]
  
  constructor(private base: Store) {
    this.pending = []
  }
  
  commit(): CommitHash {
    const hash = this.base.commit()
    for (const i of this.pending) {
      i.finish(hash)
    }
    this.pending = []
    return hash;
  }

  abort() {
    this.base.abort()
    for (const i of this.pending) {
      i.abort()
    }
    this.pending = []
  }

  all_proj(query: WhereClause, select: number[]): [WireTuple] {
    return this.base.all_proj(query, select)
  }

  all_row_id(query: WhereClause): [WireRowId] {
    return this.base.all_row_id(query)
  }

  one_proj(query: WhereClause, select: [number]): WireTuple {
    return this.base.one_proj(query, select)
  }

  // don't need one_row_id
  exists(query: WhereClause): boolean {
    return this.base.exists(query)
  }
  
  add<P extends string>(table_name: P, values: TxnWireTuple): RowId<P> {
    const wi = this.base.add(table_name, values)
    const i = new RowId(wi, table_name)
    this.pending.push(i)
    return i
  }
}
