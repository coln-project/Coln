import { Store, WireTuple } from "./store.js";
import { WhereClause } from "./types.js"
import { Adaptor } from "./flatten.js"

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
