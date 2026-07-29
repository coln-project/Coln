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
      C: (new runtime.RowIdSet.View(store, "TRealm.C", [])),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      first: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.first", [a]));
      },
      second: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.second", [a]));
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
      A: (new runtime.RowIdSet.Transaction(store, "TRealm.A", [], transaction)),
      B: (new runtime.RowIdSet.Transaction(store, "TRealm.B", [], transaction)),
      C: (new runtime.RowIdSet.Transaction(store, "TRealm.C", [], transaction)),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      },
      first: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.first",
          [a],
          transaction
        ));
      },
      second: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.second",
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