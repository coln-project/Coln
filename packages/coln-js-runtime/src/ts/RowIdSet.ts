import * as ColnSet from "./ColnSet.ts"
import { Value, tryValueRowId, Tuple, tupleEqual } from "./value.ts"
import { StoreHandle, RowView, TransactionHandle } from "#wasm-bodge/bindings"
import { toTxnValue } from "..";

export class View implements ColnSet.View {
  store: StoreHandle;
  path: string;
  params: Tuple;

  constructor(store_handle: StoreHandle, path: string, params: Tuple) {
    this.store = store_handle;
    this.path = path;
    this.params = params;
  }

  has(x: Value): boolean {
    if (x.tag !== "row_id" && x.tag !== "row_handle") return false;

    const rowId = tryValueRowId(x);
    if (rowId === undefined) return false;

    const row = this.store.rowById(this.path, rowId);

    return row !== undefined && tupleEqual(row.values, this.params);
  }

  values(): Iterator<RowView> {
    const rows = this.store.scanTable(this.path);

    return rows.filter((row) => tupleEqual(row.values, this.params)).values();
  }
}

export class Transaction extends View implements ColnSet.Transaction {
  transaction: TransactionHandle;

  constructor(store_handle: StoreHandle, path: string, params: Tuple, transaction: TransactionHandle) {
    super(store_handle, path, params);
    this.transaction = transaction;
  }
  
  add(): Value {
    return {
      tag: "row_handle",
      value: this.transaction.add(this.path, this.params.map(toTxnValue))
    };
  }
}
