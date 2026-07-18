// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { StoreHandle, type TransactionHandle } from "@coln-project/runtime";

interface RealmModule<ViewRoot, TransactionRoot> {
  schema: unknown;
  View: new (store: StoreHandle) => { root: ViewRoot };
  Transaction: new (
    store: StoreHandle,
    transaction: TransactionHandle,
  ) => { root: TransactionRoot };
}

export function beginRealm<ViewRoot, TransactionRoot>(
  realm: RealmModule<ViewRoot, TransactionRoot>,
) {
  const store = StoreHandle.fromTheory(JSON.stringify(realm.schema));
  const transaction = store.beginTransaction();
  const root = new realm.Transaction(store, transaction).root;

  return {
    root,
    commit(): ViewRoot {
      const committedStore = transaction.commit().takeStore();
      return new realm.View(committedStore).root;
    },
  };
}
