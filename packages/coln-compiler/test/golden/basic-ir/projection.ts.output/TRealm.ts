import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      r: (p: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.r", [p]));
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
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      },
      r: (p: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.r",
          [p],
          transaction
        ));
      }
    };
  }
}