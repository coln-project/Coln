import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      count: (new runtime.TableCellRef.View(store, "TRealm.count", [])),
      label: (new runtime.TableCellRef.View(store, "TRealm.label", []))
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
      count: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.count",
        [],
        transaction
      )),
      label: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.label",
        [],
        transaction
      ))
    };
  }
}