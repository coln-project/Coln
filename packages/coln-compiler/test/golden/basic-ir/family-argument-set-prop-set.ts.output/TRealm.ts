import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      A: (new runtime.RowIdSet.View(store, "TRealm.A", [])),
      B: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.B", [a]));
      },
      R: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "TRealm.R", [a, b]));
        };
      }
    };
  }
}

export class Transaction extends View {
  root: T.Transaction;

  constructor(
    store: runtime.StoreHandle,
    transaction: runtime.TransactionHandle
  ) {
    super(store);
    this.root = {
      A: (new runtime.RowIdSet.Transaction(store, "TRealm.A", [], transaction)),
      B: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.B",
          [a],
          transaction
        ));
      },
      R: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.R",
            [a, b],
            transaction
          ));
        };
      }
    };
  }
}