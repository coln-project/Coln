import { ManagedStore } from "./store.js";
import { Path, WireRowId } from "./types.js"
import { Adaptor } from "./flatten.js"
import { RowId } from "./row_id.js";
import { LiveTuple, toWire, toTxnWire } from "./value.js"

export interface Set<T> {
  values(): T[]
  contains(value: T): boolean
}

export interface MutableSet<T> extends Set<T> {
  add(): T
}

export class BaseTableSet<P extends string> implements MutableSet<RowId<P>> {
  constructor(private store: ManagedStore, private table_name: P, private bound: LiveTuple) {}
  
  values(): RowId<P>[] {
    return this.store.all_row_id({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }).map((i: WireRowId) => {
      return new RowId({ type: "Existing", value: i }, this.table_name)
    })
  }
  
  contains(v: RowId<P>): boolean {
    return this.store.exists({
      table_name: this.table_name,
      row_id: v.asWire(),
      values: this.bound.map(toWire)
    })
  }
  
  add(): RowId<P> {
    return this.store.add(this.table_name, this.bound.map(toTxnWire))
  }
}

export class ViewTableSet<T> implements Set<T> {
  constructor(private store: ManagedStore, private table_name: Path, private bound: LiveTuple, private select: number[], private adaptor: Adaptor<T>) {}

  values(): T[] {
    return this.store.all_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }, this.select).map(this.adaptor.reconstruct)
  }
  
  contains(v: T): boolean {
    return this.store.exists({
      table_name: this.table_name,
      row_id: null,
      values: [...this.bound.map(toWire), ...this.adaptor.flatten(v)]
    })
  }
}

