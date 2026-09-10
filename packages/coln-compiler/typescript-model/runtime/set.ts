import { Store, WireTuple } from "./store.js";
import { Path, WhereClause, WireRowId } from "./types.js"
import { Adaptor } from "./flatten.js"
import { RowId } from "./row_id.js";

export interface Set<T> {
  values(): T[]
  contains(value: T): boolean
}

function todo<T>(): T {
  throw "todo"
}

export interface MutableSet<T> extends Set<T> {
  add(): T
}

export class BaseTableSet<P extends string> implements MutableSet<RowId<P>> {
  constructor(private store: Store, private table_name: Path, private bound: WireTuple) {}
  
  values(): RowId<P>[] {
    return this.store.all_row_id({ table_name: this.table_name, row_id: null, values: this.bound }).map((i: WireRowId) => {return new RowId()})
  }
  
  contains(v: RowId<P>): boolean {
    return todo()
  }
  
  add(): RowId<P> {
    return todo()
  }
}

export class ViewTableSet<T> implements Set<T> {
  constructor(private store: Store, private table_name: Path, private bound: WireTuple, private select: [number], private adaptor: Adaptor<T>) {}

  values(): T[] {
    return this.store.all_proj({ table_name: this.table_name, row_id: null, values: this.bound }, this.select).map(this.adaptor.reconstruct)
  }
  
  contains(v: T): boolean {
    return this.store.exists({ table_name: this.table_name, row_id: null, values: [...this.bound, ...this.adaptor.flatten(v)] })
  }
}

