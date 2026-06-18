import { Value, StoreHandle, TransactionHandle } from "#wasm-bodge/bindings";
import { Tuple, tupleEqual } from "./tuple.ts"
import * as ColnRef from "./ColnRef";

// This assumes that the function arguments are the first n-1 elements of the
// row, and the result is the last one.
//
// We will have to update this once we have `tuple { ... } : Set` in Coln.
export class View implements ColnRef.View {
  store: StoreHandle;
  path: string;
  params: Tuple;

  constructor(store: StoreHandle, path: string, params: Tuple) {
    this.store = store;
    this.path = path;
    this.params = params;
  }
  
  get(): Value {
    const n = this.params.length;
    for (const row of this.store.scanTable(this.path)) {
      if (tupleEqual(row.values.slice(0, n), this.params)) {
        return row.values[n];
      }
    }
    throw("no such value for " + this.path + "at parameters" + JSON.stringify(this.params))
  }
}

export class Transaction extends View implements ColnRef.Transaction {
  transaction: TransactionHandle;
  
  constructor(store: StoreHandle, path: string, params: Tuple, transaction: TransactionHandle) {
    super(store, path, params)
    this.transaction = transaction;
  }

  // NOTE: this should first check if the value is already set
  // This is incorrect and lazy right now!!!
  set(v: Value): void {
    return this.transaction.add(this.path, [...this.params, v])
  }
}
