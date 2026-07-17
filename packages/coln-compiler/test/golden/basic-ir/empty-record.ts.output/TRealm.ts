import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      unit: (new runtime.TableCellRef.View(store, "TRealm.unit", []))
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
      unit: (new runtime.TableCellRef.Transaction(
        store,
        "TRealm.unit",
        [],
        transaction
      ))
    };
  }
}