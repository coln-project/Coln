// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as ColnSet from "./ColnSet"
import { Value, StoreHandle, RowView, TransactionHandle, getRowRef } from "#wasm-bodge/bindings"
import { Tuple, tupleEqual } from "./tuple"

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
    const rowRef = getRowRef(x)
    if (rowRef == undefined) return false;

    const row = this.store.rowById(this.path, rowRef);

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
    return this.transaction.add(this.path, this.params);
  }
}
