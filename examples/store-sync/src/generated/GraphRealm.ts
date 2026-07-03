// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import schema from "./GraphRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as Graph from "./Graph.ts";

export class View {
  root: Graph.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      V: (new runtime.RowIdSet.View(store, "GraphRealm.V", [])),
      E: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "GraphRealm.E", [a, b]));
        };
      }
    };
  }
}

export class Transaction extends View {
  root: Graph.Transaction;

  constructor(
    store: runtime.StoreHandle,
    transaction: runtime.TransactionHandle
  ) {
    super(store);
    this.root = {
      V: (new runtime.RowIdSet.Transaction(
        store,
        "GraphRealm.V",
        [],
        transaction
      )),
      E: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "GraphRealm.E",
            [a, b],
            transaction
          ));
        };
      }
    };
  }
}