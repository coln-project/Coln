// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { StoreHandle, TransactionHandle } from "#wasm-bodge/bindings"

export interface ColnSchema {
  entities: readonly unknown[]
  rules: readonly unknown[]
}

export interface RealmBindings<ViewRoot = unknown, TransactionRoot = unknown> {
  schema: ColnSchema
  View: new (store: StoreHandle) => { root: ViewRoot }
  Transaction: new (store: StoreHandle, transaction: TransactionHandle) => { root: TransactionRoot }
}
