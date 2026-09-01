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
      f: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.f", [a]));
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
      f: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.f",
          [a],
          transaction
        ));
      }
    };
  }
}