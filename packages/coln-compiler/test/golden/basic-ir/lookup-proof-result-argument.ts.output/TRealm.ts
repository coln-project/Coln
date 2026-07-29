import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      P: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.P", [a]));
      },
      witness: (x: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.witness", [x]));
      },
      R: (x: runtime.Value) => {
        return (a: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "TRealm.R", [x, a]));
        };
      },
      use: (x: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.use", [x]));
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
      P: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.P",
          [a],
          transaction
        ));
      },
      witness: (x: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.witness",
          [x],
          transaction
        ));
      },
      R: (x: runtime.Value) => {
        return (a: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.R",
            [x, a],
            transaction
          ));
        };
      },
      use: (x: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.use",
          [x],
          transaction
        ));
      }
    };
  }
}