// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  RowIdSet,
  type StoreHandle,
  type TransactionHandle,
  type Value,
} from "@coln-project/runtime"
export { itemSchema as schema } from "./schema"

interface Root<T> {
  Items: (value: Value) => T
}

export class View {
  root: Root<RowIdSet.View>

  constructor(store: StoreHandle) {
    this.root = {
      Items: value => new RowIdSet.View(store, "Test.Items", [value]),
    }
  }
}

export class Transaction extends View {
  declare root: Root<RowIdSet.Transaction>

  constructor(store: StoreHandle, transaction: TransactionHandle) {
    super(store)
    this.root = {
      Items: value => new RowIdSet.Transaction(store, "Test.Items", [value], transaction),
    }
  }
}
