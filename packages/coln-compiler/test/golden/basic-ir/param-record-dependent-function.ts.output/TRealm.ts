import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      Key: (new runtime.RowIdSet.View(store, "TRealm.Key", [])),
      key: (new runtime.TableCellRef.View(store, "TRealm.key", [])),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.E", [a]));
      },
      f: (x: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.f", [x]));
      },
      point: (new runtime.TableCellRef.View(store, "TRealm.point", [])),
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
      key: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.key",
        [],
        transaction
      )),
      E: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.E",
          [a],
          transaction
        ));
      },
      f: (x: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.f",
          [x],
          transaction
        ));
      },
      point: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.point",
        [],
        transaction
      )),
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