import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      rank: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.rank", [a]));
      },
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      edge: (x: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.edge", [x]));
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
      rank: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.rank",
          [a],
          transaction
        ));
      },
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      },
      edge: (x: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.edge",
          [x],
          transaction
        ));
      }
    };
  }
}