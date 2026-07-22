import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      P: (new runtime.RowIdSet.View(store, "TRealm.P", [])),
      Y: (new runtime.RowIdSet.View(store, "TRealm.Y", [])),
      choose: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.TableCellRef.View(
            store,
            "TRealm.choose",
            [a, b]
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
      P: (new runtime.RowIdSet.Transaction(store, "TRealm.P", [], transaction)),
      Y: (new runtime.RowIdSet.Transaction(store, "TRealm.Y", [], transaction)),
      choose: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.TableCellRef.Transaction(
            store,
            "TRealm.choose",
            [a, b],
            transaction
          ));
        };
      }
    };
  }
}