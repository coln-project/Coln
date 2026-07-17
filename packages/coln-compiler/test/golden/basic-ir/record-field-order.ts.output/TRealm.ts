import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      E: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "TRealm.E", [a, b]));
        };
      },
      R: (p: runtime.Value) => {
        return (a: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "TRealm.R", [p, a]));
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
      E: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.E",
            [a, b],
            transaction
          ));
        };
      },
      R: (p: runtime.Value) => {
        return (a: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.R",
            [p, a],
            transaction
          ));
        };
      }
    };
  }
}