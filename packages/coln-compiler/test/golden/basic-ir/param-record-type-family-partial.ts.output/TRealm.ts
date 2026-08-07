import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      X: (new runtime.RowIdSet.View(store, "TRealm.X", [])),
      Y: (new runtime.RowIdSet.View(store, "TRealm.Y", [])),
      R: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.View(store, "TRealm.R", [a, b]));
        };
      },
      slices: (x: runtime.Value) => {
        return (y: runtime.Value) => {
          return (a: runtime.Value) => {
            return (new runtime.RowIdSet.View(
              store,
              "TRealm.slices",
              [x, y, a]
            ));
          };
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
      Y: (new runtime.RowIdSet.Transaction(store, "TRealm.Y", [], transaction)),
      R: (a: runtime.Value) => {
        return (b: runtime.Value) => {
          return (new runtime.RowIdSet.Transaction(
            store,
            "TRealm.R",
            [a, b],
            transaction
          ));
        };
      },
      slices: (x: runtime.Value) => {
        return (y: runtime.Value) => {
          return (a: runtime.Value) => {
            return (new runtime.RowIdSet.Transaction(
              store,
              "TRealm.slices",
              [x, y, a],
              transaction
            ));
          };
        };
      }
    };
  }
}