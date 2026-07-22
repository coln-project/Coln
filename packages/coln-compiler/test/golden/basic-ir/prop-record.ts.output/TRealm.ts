import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      P: (new runtime.RowIdSet.View(store, "TRealm.P", [])),
      Q: (new runtime.RowIdSet.View(store, "TRealm.Q", [])),
      make: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.TableCellRef.View(store, "TRealm.make", [a, b]));
        };
      },
      projectLeft: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(
          store,
          "TRealm.projectLeft",
          [a]
        ));
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
      P: (new runtime.RowIdSet.Transaction(store, "TRealm.P", [], transaction)),
      Q: (new runtime.RowIdSet.Transaction(store, "TRealm.Q", [], transaction)),
      make: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.TableCellRef.Transaction(
            store,
            "TRealm.make",
            [a, b],
            transaction
          ));
        };
      },
      projectLeft: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.projectLeft",
          [a],
          transaction
        ));
      }
    };
  }
}