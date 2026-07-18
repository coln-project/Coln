import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      Pair: (x: runtime.Value) => {
        return (x_slash_a: runtime.Value) => {
          return (new runtime.RowIdSet.View(
            store,
            "TRealm.Pair",
            [x, x_slash_a]
          ));
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
      X: (new runtime.RowIdSet.Transaction(store, "TRealm.X", [], transaction)),
      Pair: (x: runtime.Value) => {
        return (x_slash_a: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.Pair",
            [x, x_slash_a],
            transaction
          ));
        };
      }
    };
  }
}