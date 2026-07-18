import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      point: (new runtime.RowIdSet.View(store, "TRealm.point", [])),
      payload: (a: runtime.Value) => {
        return (new runtime.TableCellRef.View(store, "TRealm.payload", [a]));
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
      point: (new runtime.RowIdSet.Transaction(
        store,
        "TRealm.point",
        [],
        transaction
      )),
      payload: (a: runtime.Value) => {
        return (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.payload",
          [a],
          transaction
        ));
      }
    };
  }
}