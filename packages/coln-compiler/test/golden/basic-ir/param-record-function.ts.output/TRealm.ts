import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      Key: (new runtime.RowIdSet.View(store, "TRealm.Key", [])),
      f: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.f", [a]));
      },
      boxed: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.boxed", [a]));
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
      Key: (new runtime.RowIdSet.Transaction(
        store,
        "TRealm.Key",
        [],
        transaction
      )),
      f: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.f",
          [a],
          transaction
        ));
      },
      boxed: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.boxed",
          [a],
          transaction
        ));
      }
    };
  }
}