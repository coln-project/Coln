import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      P: (a: runtime.Value) => {
        return (new runtime.RowIdSet.View(store, "TRealm.P", [a]));
      },
      package: (new runtime.TableCellRef.View(store, "TRealm.package", []))
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
      P: (a: runtime.Value) => {
        return (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.P",
          [a],
          transaction
        ));
      },
      package: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.package",
        [],
        transaction
      ))
    };
  }
}