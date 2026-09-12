import { Adaptor } from "./flatten"
import { ManagedStore } from "./store"
import { Path } from "./types"
import { LiveTuple, toWire } from "./value"

export interface Ref<T> {
  value(): T
}

export interface MutableRef<T> extends Ref<T> {
  set(val: T): void
}

export class BaseTableRef<T> implements MutableRef<T> {
  constructor(private store: ManagedStore, private table_name: Path, private bound: LiveTuple, private select: [number], private adaptor: Adaptor<T>) {}
  
  value(): T {
    return this.adaptor.reconstruct(this.store.one_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }, this.select))
  }
  
  set(val: T): void {
    this.store.add(this.table_name, [...this.bound.map(toWire), ...this.adaptor.flatten(val)])
  }
}
