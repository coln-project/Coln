import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as Base from "./Base.ts";
import * as Alias from "./Alias.ts";

export class View {
  root: Base.View;

  constructor(store: runtime.StoreHandle) {
    this.root = { X: (new runtime.RowIdSet.View(store, "TRealm.X", [])) };
  }
}

export class Transaction extends View {
  root: Base.Transaction;

  constructor(
    store: runtime.StoreHandle,
    transaction: runtime.TransactionHandle
  ) {
    super(store);
    this.root = {
      X: (new runtime.RowIdSet.Transaction(store, "TRealm.X", [], transaction))
    };
  }
}