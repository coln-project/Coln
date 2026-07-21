import schema from "./TRealm.json";
export {schema};
import * as runtime from "@coln-project/runtime";
import * as Model from "./Model.ts";
import * as PointOf from "./PointOf.ts";
import * as T from "./T.ts";

export class View {
  root: T.View;

  constructor(store: runtime.StoreHandle) {
    this.root = {
      model: { X: (new runtime.RowIdSet.View(store, "TRealm.model.X", [])) },
      pointed: {
        point: (new runtime.TableCellRef.View(
          store,
          "TRealm.pointed.point",
          []
        ))
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
      pointed: {
        point: (new runtime.TableCellRef.Transaction(
          store,
          "TRealm.pointed.point",
          [],
          transaction
        ))
      }
    };
  }
}