import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as Model from "./Model.ts";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      model: { X: (new runtime.RowIdSet.View(store, "TRealm.model.X", [])) },
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
      model: {
        X: (new runtime.RowIdSet.Transaction(
          store,
          "TRealm.model.X",
          [],
          transaction
        ))
      },
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