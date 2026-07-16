import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      V: (new runtime.RowIdSet.View(store, "TRealm.V", [])),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
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
      V: (new runtime.RowIdSet.Transaction(store, "TRealm.V", [], transaction)),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      }
    };
  }
}