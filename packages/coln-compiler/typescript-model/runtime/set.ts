import { Store, WireTuple } from "./store.js";
import { WhereClause } from "./types.js"
import { Adaptor } from "./flatten.js"

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

export class BoundBaseTable<T> implements MutableSet<T> {
  constructor(private store: Store) {}
  
  values(): T[] {
    return todo()
  }
  
  contains(v: T): boolean {
    return todo()
  }
  
  add(): T {
    return todo()
  }
}

export class View<T> {
  constructor(
    private store: Store,
    private table_name: WhereClause,
    private params: WireTuple,
    private adapter: Adaptor<T>
  ) {}
  
  // values(): T[] {
  //   return this.store.all(this.where, this.select).map(this.reconstruct)
  // }
}
