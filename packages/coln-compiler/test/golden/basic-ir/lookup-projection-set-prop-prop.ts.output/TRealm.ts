import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      A: (new runtime.RowIdSet.View(store, "TRealm.A", [])),
      B: (new runtime.RowIdSet.View(store, "TRealm.B", [])),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      next: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.next", [a]));
      },
      nextedge: (x: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.nextedge", [x]));
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
      B: (new runtime.RowIdSet.Transaction(store, "TRealm.B", [], transaction)),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      },
      next: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.next",
          [a],
          transaction
        ));
      },
      nextedge: (x: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.nextedge",
          [x],
          transaction
        ));
      }
    };
  }
}